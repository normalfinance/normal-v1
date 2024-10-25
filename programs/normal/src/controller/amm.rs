use std::cmp::{ max, min, Ordering };

use anchor_lang::prelude::*;
use solana_program::msg;

use crate::controller::position::PositionDirection;
use crate::controller::repeg::apply_cost_to_market;
use crate::error::{ NormalResult, ErrorCode };
use crate::get_then_update_id;
use crate::math::amm::calculate_quote_asset_amount_swapped;
use crate::math::amm_spread::{ calculate_spread_reserves, get_spread_reserves };
use crate::math::casting::Cast;
use crate::math::constants::{
	CONCENTRATION_PRECISION,
	FEE_POOL_TO_REVENUE_POOL_THRESHOLD,
	K_BPS_UPDATE_SCALE,
	MAX_CONCENTRATION_COEFFICIENT,
	MAX_K_BPS_INCREASE,
	MAX_SQRT_K,
};
use crate::math::cp_curve::get_update_k_result;
use crate::math::repeg::get_total_fee_lower_bound;
use crate::math::safe_math::SafeMath;
use crate::math::balance::get_token_amount;
use crate::math::{ amm, amm_spread, bn, cp_curve, quote_asset::* };

use crate::state::events::CurveRecord;
use crate::state::oracle::OraclePriceData;
use crate::state::amm::AMM;
use crate::state::market::{ Balance, Market };
use crate::state::user::{ Position, User };
use crate::validate;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwapDirection {
	Add,
	Remove,
}

fn calculate_quote_asset_amount_surplus(
	quote_asset_reserve_before: u128,
	quote_asset_reserve_after: u128,
	swap_direction: SwapDirection,
	peg_multiplier: u128,
	initial_quote_asset_amount: u128,
	round_down: bool
) -> NormalResult<u128> {
	let quote_asset_reserve_change = match swap_direction {
		SwapDirection::Add =>
			quote_asset_reserve_before.safe_sub(quote_asset_reserve_after)?,

		SwapDirection::Remove =>
			quote_asset_reserve_after.safe_sub(quote_asset_reserve_before)?,
	};

	let mut actual_quote_asset_amount = reserve_to_asset_amount(
		quote_asset_reserve_change,
		peg_multiplier
	)?;

	// Compensate for +1 quote asset amount added when removing base asset
	if round_down {
		actual_quote_asset_amount = actual_quote_asset_amount.safe_add(1)?;
	}

	let quote_asset_amount_surplus = if
		actual_quote_asset_amount > initial_quote_asset_amount
	{
		actual_quote_asset_amount.safe_sub(initial_quote_asset_amount)?
	} else {
		initial_quote_asset_amount.safe_sub(actual_quote_asset_amount)?
	};

	Ok(quote_asset_amount_surplus)
}

pub fn swap_base_asset(
	market: &mut Market,
	base_asset_swap_amount: u64,
	direction: SwapDirection
) -> NormalResult<(u64, i64)> {
	let (
		new_base_asset_reserve,
		new_quote_asset_reserve,
		quote_asset_amount,
		quote_asset_amount_surplus,
	) = calculate_base_swap_output_with_spread(
		&market.amm,
		base_asset_swap_amount,
		direction
	)?;

	market.amm.base_asset_reserve = new_base_asset_reserve;
	market.amm.quote_asset_reserve = new_quote_asset_reserve;

	Ok((quote_asset_amount, quote_asset_amount_surplus.cast::<i64>()?))
}

pub fn calculate_base_swap_output_with_spread(
	amm: &AMM,
	base_asset_swap_amount: u64,
	direction: SwapDirection
) -> NormalResult<(u128, u128, u64, u64)> {
	// first do the swap with spread reserves to figure out how much base asset is acquired
	let (base_asset_reserve_with_spread, quote_asset_reserve_with_spread) =
		get_spread_reserves(amm, match direction {
			SwapDirection::Add => PositionDirection::Short,
			SwapDirection::Remove => PositionDirection::Long,
		})?;

	let (new_quote_asset_reserve_with_spread, _) = amm::calculate_swap_output(
		base_asset_swap_amount.cast()?,
		base_asset_reserve_with_spread,
		direction,
		amm.sqrt_k
	)?;

	let quote_asset_amount = calculate_quote_asset_amount_swapped(
		quote_asset_reserve_with_spread,
		new_quote_asset_reserve_with_spread,
		direction,
		amm.peg_multiplier
	)?;

	let (new_quote_asset_reserve, new_base_asset_reserve) =
		amm::calculate_swap_output(
			base_asset_swap_amount.cast()?,
			amm.base_asset_reserve,
			direction,
			amm.sqrt_k
		)?;

	// calculate the quote asset surplus by taking the difference between what quote_asset_amount is
	// with and without spread
	let quote_asset_amount_surplus = calculate_quote_asset_amount_surplus(
		new_quote_asset_reserve,
		amm.quote_asset_reserve,
		match direction {
			SwapDirection::Remove => SwapDirection::Add,
			SwapDirection::Add => SwapDirection::Remove,
		},
		amm.peg_multiplier,
		quote_asset_amount,
		direction == SwapDirection::Remove
	)?;

	Ok((
		new_base_asset_reserve,
		new_quote_asset_reserve,
		quote_asset_amount.cast::<u64>()?,
		quote_asset_amount_surplus.cast::<u64>()?,
	))
}

pub fn update_spread_reserves(market: &mut Market) -> NormalResult {
	let (new_ask_base_asset_reserve, new_ask_quote_asset_reserve) =
		calculate_spread_reserves(market, PositionDirection::Long)?;
	let (new_bid_base_asset_reserve, new_bid_quote_asset_reserve) =
		calculate_spread_reserves(market, PositionDirection::Short)?;

	market.amm.ask_base_asset_reserve = new_ask_base_asset_reserve.min(
		market.amm.base_asset_reserve
	);
	market.amm.bid_base_asset_reserve = new_bid_base_asset_reserve.max(
		market.amm.base_asset_reserve
	);
	market.amm.ask_quote_asset_reserve = new_ask_quote_asset_reserve.max(
		market.amm.quote_asset_reserve
	);
	market.amm.bid_quote_asset_reserve = new_bid_quote_asset_reserve.min(
		market.amm.quote_asset_reserve
	);

	Ok(())
}

pub fn update_spreads(
	market: &mut Market,
	reserve_price: u64
) -> NormalResult<(u32, u32)> {
	let max_ref_offset = market.amm.get_max_reference_price_offset()?;

	let reference_price_offset = if max_ref_offset > 0 {
		let liquidity_ratio = amm_spread::calculate_inventory_liquidity_ratio(
			market.amm.base_asset_amount_with_amm,
			market.amm.base_asset_reserve,
			market.amm.max_base_asset_reserve,
			market.amm.min_base_asset_reserve
		)?;

		let signed_liquidity_ratio = liquidity_ratio.safe_mul(
			market.amm.get_protocol_owned_position()?.signum().cast()?
		)?;

		amm_spread::calculate_reference_price_offset(
			reserve_price,
			signed_liquidity_ratio,
			market.amm.min_order_size,
			market.amm.historical_oracle_data.last_oracle_price_twap_5min,
			market.amm.last_mark_price_twap_5min,
			market.amm.historical_oracle_data.last_oracle_price_twap,
			market.amm.last_mark_price_twap,
			max_ref_offset
		)?
	} else {
		0
	};

	let (long_spread, short_spread) = if market.amm.curve_update_intensity > 0 {
		amm_spread::calculate_spread(
			market.amm.base_spread,
			market.amm.last_oracle_reserve_price_spread_pct,
			market.amm.last_oracle_conf_pct,
			market.amm.max_spread,
			market.amm.quote_asset_reserve,
			market.amm.terminal_quote_asset_reserve,
			market.amm.peg_multiplier,
			market.amm.base_asset_amount_with_amm,
			reserve_price,
			market.amm.total_fee_minus_distributions,
			market.amm.base_asset_reserve,
			market.amm.min_base_asset_reserve,
			market.amm.max_base_asset_reserve,
			market.amm.mark_std,
			market.amm.oracle_std,
			market.amm.long_intensity_volume,
			market.amm.short_intensity_volume,
			market.amm.volume_24h
		)?
	} else {
		let half_base_spread = market.amm.base_spread.safe_div(2)?;
		(half_base_spread, half_base_spread)
	};

	market.amm.long_spread = long_spread;
	market.amm.short_spread = short_spread;
	market.amm.reference_price_offset = reference_price_offset;

	update_spread_reserves(market)?;

	Ok((long_spread, short_spread))
}

pub fn update_concentration_coef(
	market: &mut Market,
	scale: u128
) -> NormalResult {
	validate!(scale > 0, ErrorCode::InvalidConcentrationCoef, "invalid scale")?;

	let new_concentration_coef =
		CONCENTRATION_PRECISION +
		(MAX_CONCENTRATION_COEFFICIENT - CONCENTRATION_PRECISION) / scale;

	validate!(
		new_concentration_coef > CONCENTRATION_PRECISION &&
			new_concentration_coef <= MAX_CONCENTRATION_COEFFICIENT,
		ErrorCode::InvalidConcentrationCoef,
		"invalid new_concentration_coef"
	)?;

	market.amm.concentration_coef = new_concentration_coef;

	let (_, terminal_quote_reserves, terminal_base_reserves) =
		amm::calculate_terminal_price_and_reserves(&market.amm)?;

	validate!(
		terminal_quote_reserves == market.amm.terminal_quote_asset_reserve,
		ErrorCode::InvalidAmmDetected,
		"invalid terminal_quote_reserves"
	)?;

	// updating the concentration_coef changes the min/max base_asset_reserve
	// doing so adds ability to improve amm constant product curve's slippage
	// by increasing k as same factor as scale w/o increasing imbalance risk
	let (min_base_asset_reserve, max_base_asset_reserve) =
		amm::calculate_bid_ask_bounds(
			market.amm.concentration_coef,
			terminal_base_reserves
		)?;

	market.amm.max_base_asset_reserve = max_base_asset_reserve;
	market.amm.min_base_asset_reserve = min_base_asset_reserve;

	let reserve_price_after = market.amm.reserve_price()?;
	update_spreads(market, reserve_price_after)?;

	let (max_bids, max_asks) = amm::calculate_market_open_bids_asks(&market.amm)?;
	validate!(
		max_bids > market.amm.base_asset_amount_with_amm &&
			max_asks < market.amm.base_asset_amount_with_amm,
		ErrorCode::InvalidConcentrationCoef,
		"amm.base_asset_amount_with_amm exceeds the unload liquidity available after concentration adjustment"
	)?;

	Ok(())
}

pub fn get_fee_pool_tokens(market: &mut Market) -> NormalResult<i128> {
	get_token_amount(market.amm.fee_pool.balance(), market)?.cast()
}

pub fn move_price(
	market: &mut Market,
	base_asset_reserve: u128,
	quote_asset_reserve: u128,
	sqrt_k: u128
) -> NormalResult {
	market.amm.base_asset_reserve = base_asset_reserve;

	let k = bn::U256::from(sqrt_k).safe_mul(bn::U256::from(sqrt_k))?;

	market.amm.quote_asset_reserve = k
		.safe_div(bn::U256::from(base_asset_reserve))?
		.try_to_u128()?;

	validate!(
		(
			quote_asset_reserve.cast::<i128>()? -
			market.amm.quote_asset_reserve.cast::<i128>()?
		).abs() < 100,
		ErrorCode::InvalidAmmDetected,
		"quote_asset_reserve passed doesnt reconcile enough {} vs {}",
		quote_asset_reserve.cast::<i128>()?,
		market.amm.quote_asset_reserve.cast::<i128>()?
	)?;

	market.amm.sqrt_k = sqrt_k;

	let (_, terminal_quote_reserves, terminal_base_reserves) =
		amm::calculate_terminal_price_and_reserves(&market.amm)?;
	market.amm.terminal_quote_asset_reserve = terminal_quote_reserves;

	let (min_base_asset_reserve, max_base_asset_reserve) =
		amm::calculate_bid_ask_bounds(
			market.amm.concentration_coef,
			terminal_base_reserves
		)?;

	market.amm.max_base_asset_reserve = max_base_asset_reserve;
	market.amm.min_base_asset_reserve = min_base_asset_reserve;

	let reserve_price_after = market.amm.reserve_price()?;
	update_spreads(market, reserve_price_after)?;

	Ok(())
}

// recenter peg with balanced terminal reserves
pub fn recenter_market_amm(
	market: &mut Market,
	peg_multiplier: u128,
	sqrt_k: u128
) -> NormalResult {
	// calculate base/quote reserves for balanced terminal reserves
	let swap_direction = if market.amm.base_asset_amount_with_amm > 0 {
		SwapDirection::Remove
	} else {
		SwapDirection::Add
	};
	let (new_quote_asset_amount, new_base_asset_amount) =
		amm::calculate_swap_output(
			market.amm.base_asset_amount_with_amm.unsigned_abs(),
			sqrt_k,
			swap_direction,
			sqrt_k
		)?;

	market.amm.base_asset_reserve = new_base_asset_amount;

	let k = bn::U256::from(sqrt_k).safe_mul(bn::U256::from(sqrt_k))?;

	market.amm.quote_asset_reserve = k
		.safe_div(bn::U256::from(new_base_asset_amount))?
		.try_to_u128()?;

	validate!(
		(
			new_quote_asset_amount.cast::<i128>()? -
			market.amm.quote_asset_reserve.cast::<i128>()?
		).abs() < 100,
		ErrorCode::InvalidAmmDetected,
		"quote_asset_reserve passed doesnt reconcile enough"
	)?;

	market.amm.sqrt_k = sqrt_k;
	// todo: could calcualte terminal state cost for altering sqrt_k

	market.amm.peg_multiplier = peg_multiplier;

	let (_, terminal_quote_reserves, terminal_base_reserves) =
		amm::calculate_terminal_price_and_reserves(&market.amm)?;
	market.amm.terminal_quote_asset_reserve = terminal_quote_reserves;

	let (min_base_asset_reserve, max_base_asset_reserve) =
		amm::calculate_bid_ask_bounds(
			market.amm.concentration_coef,
			terminal_base_reserves
		)?;

	market.amm.max_base_asset_reserve = max_base_asset_reserve;
	market.amm.min_base_asset_reserve = min_base_asset_reserve;

	let reserve_price_after = market.amm.reserve_price()?;
	update_spreads(market, reserve_price_after)?;

	Ok(())
}

// recalculate and update summary stats on amm which are prone too accumulating integer math errors
pub fn calculate_market_amm_summary_stats(
	market: &Market,
	market_oracle_price: i64
) -> NormalResult<i128> {
	let pnl_pool_token_amount = get_token_amount(
		market.pnl_pool.balance,
		market
	)?;

	let fee_pool_token_amount = get_token_amount(
		market.amm.fee_pool.balance,
		market
	)?;

	let pnl_tokens_available: i128 = pnl_pool_token_amount
		.safe_add(fee_pool_token_amount)?
		.cast()?;

	let net_user_pnl = amm::calculate_net_user_pnl(
		&market.amm,
		market_oracle_price
	)?;

	// amm's mm_fee can be incorrect with drifting integer math error
	let new_total_fee_minus_distributions =
		pnl_tokens_available.safe_sub(net_user_pnl)?;

	Ok(new_total_fee_minus_distributions)
}
