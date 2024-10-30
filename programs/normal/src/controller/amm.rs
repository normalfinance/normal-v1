use std::cmp::{ max, min, Ordering };

use anchor_lang::prelude::*;
use solana_program::msg;

use crate::controller::position::OrderSide;
use crate::controller::repeg::apply_cost_to_market;
use crate::error::{ NormalResult, ErrorCode };
use crate::get_then_update_id;
use crate::math::amm::calculate_quote_asset_amount_swapped;
use crate::math::amm_spread::{ calculate_spread_reserves, get_spread_reserves };
use crate::math::casting::Cast;
use crate::constants::constants::{
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
			SwapDirection::Add => OrderSide::Sell,
			SwapDirection::Remove => OrderSide::Buy,
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
		calculate_spread_reserves(market, OrderSide::Buy)?;
	let (new_bid_base_asset_reserve, new_bid_quote_asset_reserve) =
		calculate_spread_reserves(market, OrderSide::Sell)?;

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
			market.amm.last_mark_price_twap,
			// market.amm.historical_oracle_data.last_oracle_price_twap,
			// market.amm.last_mark_price_twap,
			max_ref_offset
		)?
	} else {
		0
	};

	let (buy_spread, sell_spread) = if market.amm.curve_update_intensity > 0 {
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
			market.amm.buy_intensity_volume,
			market.amm.sell_intensity_volume,
			market.amm.volume_24h
		)?
	} else {
		let half_base_spread = market.amm.base_spread.safe_div(2)?;
		(half_base_spread, half_base_spread)
	};

	market.amm.buy_spread = buy_spread;
	market.amm.sell_spread = sell_spread;
	market.amm.reference_price_offset = reference_price_offset;

	update_spread_reserves(market)?;

	Ok((buy_spread, sell_spread))
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

// TODO: review these functions

fn calculate_revenue_pool_transfer(
	market: &PerpMarket,
	spot_market: &SpotMarket,
	amm_fee_pool_token_amount_after: u128,
	terminal_state_surplus: i128
) -> DriftResult<i128> {
	// Calculates the revenue pool transfer amount for a given market state (positive = send to revenue pool, negative = pull from revenue pool)
	// If the AMM budget is above `FEE_POOL_TO_REVENUE_POOL_THRESHOLD` (in surplus), settle fees collected to the revenue pool depending on the health of the AMM state
	// Otherwise, spull from the revenue pool (up to a constraint amount)

	let amm_budget_surplus = terminal_state_surplus.saturating_sub(
		FEE_POOL_TO_REVENUE_POOL_THRESHOLD.cast()?
	);

	if amm_budget_surplus > 0 {
		let fee_pool_threshold = amm_fee_pool_token_amount_after
			.saturating_sub(
				FEE_POOL_TO_REVENUE_POOL_THRESHOLD.safe_add(
					market.amm.total_social_loss
				)?.cast()?
			)
			.cast()?;

		let total_liq_fees_for_revenue_pool = market.amm.total_liquidation_fee
			.min(
				market.insurance_claim.quote_settled_insurance
					.safe_add(market.insurance_claim.quote_max_insurance)?
					.cast()?
			)
			.cast::<i128>()?;

		let max_revenue_to_settle =
			market.insurance_claim.revenue_withdraw_since_last_settle
				.safe_add(
					market.insurance_claim.max_revenue_withdraw_per_period.cast()?
				)?
				.min(market.amm.net_revenue_since_last_funding)
				.max(0);

		let total_fee_for_if = get_total_fee_lower_bound(market)?.cast::<i128>()?;

		let revenue_pool_transfer = total_fee_for_if
			.safe_add(total_liq_fees_for_revenue_pool)?
			.saturating_sub(market.amm.total_fee_withdrawn.cast()?)
			.max(0)
			.min(fee_pool_threshold)
			.min(max_revenue_to_settle.cast()?);

		validate!(
			revenue_pool_transfer >= 0,
			ErrorCode::InsufficientPerpPnlPool,
			"revenue_pool_transfer negative ({})",
			revenue_pool_transfer
		)?;

		Ok(revenue_pool_transfer)
	} else if amm_budget_surplus < 0 {
		let max_revenue_withdraw_allowed =
			market.insurance_claim.max_revenue_withdraw_per_period
				.cast::<i64>()?
				.saturating_sub(
					market.insurance_claim.revenue_withdraw_since_last_settle
				)
				.cast::<u128>()?
				.min(
					get_token_amount(
						spot_market.revenue_pool.scaled_balance,
						spot_market,
						&SpotBalanceType::Deposit
					)?.cast()?
				)
				.min(market.insurance_claim.max_revenue_withdraw_per_period.cast()?);

		if max_revenue_withdraw_allowed > 0 {
			let revenue_pool_transfer = -amm_budget_surplus
				.abs()
				.min(max_revenue_withdraw_allowed.cast()?);
			Ok(revenue_pool_transfer)
		} else {
			Ok(0)
		}
	} else {
		Ok(0)
	}
}

pub fn update_pool_balances(
	market: &mut PerpMarket,
	spot_market: &mut SpotMarket,
	user_quote_position: &SpotPosition,
	user_unsettled_pnl: i128,
	now: i64
) -> DriftResult<i128> {
	// current spot_market balance of amm fee pool
	let amm_fee_pool_token_amount = get_token_amount(
		market.amm.fee_pool.balance(),
		spot_market,
		market.amm.fee_pool.balance_type()
	)?.cast::<i128>()?;

	let mut fraction_for_amm = 100;

	let amm_target_max_fee_pool_token_amount =
		market.amm.total_fee_minus_distributions
			.safe_add(market.amm.total_liquidation_fee.cast()?)?
			.safe_sub(market.amm.total_fee_withdrawn.cast()?)?;

	if amm_target_max_fee_pool_token_amount <= amm_fee_pool_token_amount {
		// owe the market pnl pool before settling user
		let pnl_pool_addition = max(
			0,
			amm_target_max_fee_pool_token_amount
		).safe_sub(amm_fee_pool_token_amount)?;

		if pnl_pool_addition < 0 {
			transfer_spot_balances(
				pnl_pool_addition.abs(),
				spot_market,
				&mut market.amm.fee_pool,
				&mut market.pnl_pool
			)?;
		}

		fraction_for_amm = 0;
	}

	{
		let amm_target_min_fee_pool_token_amount = get_total_fee_lower_bound(
			market
		)?
			.safe_add(market.amm.total_liquidation_fee)?
			.safe_sub(market.amm.total_fee_withdrawn)?;

		let amm_fee_pool_token_amount = get_token_amount(
			market.amm.fee_pool.balance(),
			spot_market,
			market.amm.fee_pool.balance_type()
		)?;

		if amm_fee_pool_token_amount < amm_target_min_fee_pool_token_amount {
			let pnl_pool_token_amount = get_token_amount(
				market.pnl_pool.balance(),
				spot_market,
				market.pnl_pool.balance_type()
			)?;

			let pnl_pool_removal = amm_target_min_fee_pool_token_amount
				.safe_sub(amm_fee_pool_token_amount)?
				.min(pnl_pool_token_amount);

			if pnl_pool_removal > 0 {
				transfer_spot_balances(
					pnl_pool_removal.cast::<i128>()?,
					spot_market,
					&mut market.pnl_pool,
					&mut market.amm.fee_pool
				)?;
			}
		}

		let amm_fee_pool_token_amount_after = get_token_amount(
			market.amm.fee_pool.balance(),
			spot_market,
			market.amm.fee_pool.balance_type()
		)?;

		let terminal_state_surplus =
			market.amm.total_fee_minus_distributions.safe_sub(
				market.amm.total_fee_withdrawn.cast()?
			)?;

		// market can perform withdraw from revenue pool
		if
			spot_market.insurance_fund.last_revenue_settle_ts >
			market.insurance_claim.last_revenue_withdraw_ts
		{
			validate!(
				now >= market.insurance_claim.last_revenue_withdraw_ts &&
					now >= spot_market.insurance_fund.last_revenue_settle_ts,
				ErrorCode::BlockchainClockInconsistency,
				"issue with clock unix timestamp {} < market.insurance_claim.last_revenue_withdraw_ts={}/spot_market.last_revenue_settle_ts={}",
				now,
				market.insurance_claim.last_revenue_withdraw_ts,
				spot_market.insurance_fund.last_revenue_settle_ts
			)?;
			market.insurance_claim.revenue_withdraw_since_last_settle = 0;
		}

		let revenue_pool_transfer = calculate_revenue_pool_transfer(
			market,
			spot_market,
			amm_fee_pool_token_amount_after,
			terminal_state_surplus
		)?;

		match revenue_pool_transfer.cmp(&0) {
			Ordering::Greater => {
				transfer_spot_balance_to_revenue_pool(
					revenue_pool_transfer.unsigned_abs(),
					spot_market,
					&mut market.amm.fee_pool
				)?;

				market.amm.total_fee_withdrawn =
					market.amm.total_fee_withdrawn.safe_add(
						revenue_pool_transfer.unsigned_abs()
					)?;
			}
			Ordering::Less => {
				transfer_revenue_pool_to_spot_balance(
					revenue_pool_transfer.unsigned_abs(),
					spot_market,
					&mut market.amm.fee_pool
				)?;
			}
			Ordering::Equal => (),
		}

		if revenue_pool_transfer != 0 {
			market.amm.total_fee_minus_distributions =
				market.amm.total_fee_minus_distributions.safe_sub(
					revenue_pool_transfer
				)?;

			market.insurance_claim.revenue_withdraw_since_last_settle =
				market.insurance_claim.revenue_withdraw_since_last_settle.safe_sub(
					revenue_pool_transfer.cast()?
				)?;
			market.insurance_claim.last_revenue_withdraw_ts = now;
		}
	}

	// market pnl pool pays (what it can to) user_unsettled_pnl and pnl_to_settle_to_amm
	let pnl_pool_token_amount = get_token_amount(
		market.pnl_pool.balance(),
		spot_market,
		market.pnl_pool.balance_type()
	)?;

	let pnl_to_settle_with_user = if user_unsettled_pnl > 0 {
		min(user_unsettled_pnl, pnl_pool_token_amount.cast::<i128>()?)
	} else {
		let token_amount =
			user_quote_position.get_signed_token_amount(spot_market)?;

		// dont settle negative pnl to spot borrows when utilization is high (> 80%)
		let max_withdraw_amount = -get_max_withdraw_for_market_with_token_amount(
			spot_market,
			token_amount,
			false
		)?.cast::<i128>()?;

		max_withdraw_amount.max(user_unsettled_pnl)
	};

	let pnl_fraction_for_amm = if
		fraction_for_amm > 0 &&
		pnl_to_settle_with_user < 0
	{
		let pnl_fraction_for_amm =
			pnl_to_settle_with_user.safe_div(fraction_for_amm)?;
		update_spot_balances(
			pnl_fraction_for_amm.unsigned_abs(),
			&SpotBalanceType::Deposit,
			spot_market,
			&mut market.amm.fee_pool,
			false
		)?;
		pnl_fraction_for_amm
	} else {
		0
	};

	let pnl_to_settle_with_market =
		-pnl_to_settle_with_user.safe_sub(pnl_fraction_for_amm)?;

	update_spot_balances(
		pnl_to_settle_with_market.unsigned_abs(),
		if pnl_to_settle_with_market >= 0 {
			&SpotBalanceType::Deposit
		} else {
			&SpotBalanceType::Borrow
		},
		spot_market,
		&mut market.pnl_pool,
		false
	)?;

	let _depositors_claim = validate_spot_balances(spot_market)?;

	Ok(pnl_to_settle_with_user)
}

pub fn update_pnl_pool_and_user_balance(
	market: &mut PerpMarket,
	bank: &mut SpotMarket,
	user: &mut User,
	unrealized_pnl_with_fee: i128
) -> DriftResult<i128> {
	let pnl_to_settle_with_user = if unrealized_pnl_with_fee > 0 {
		unrealized_pnl_with_fee.min(
			get_token_amount(
				market.pnl_pool.scaled_balance,
				bank,
				market.pnl_pool.balance_type()
			)?.cast()?
		)
	} else {
		unrealized_pnl_with_fee
	};

	validate!(
		unrealized_pnl_with_fee == pnl_to_settle_with_user,
		ErrorCode::InsufficientPerpPnlPool,
		"pnl_pool_amount doesnt have enough ({} < {})",
		pnl_to_settle_with_user,
		unrealized_pnl_with_fee
	)?;

	if unrealized_pnl_with_fee == 0 {
		msg!("User has no unsettled pnl for market {}", market.market_index);
		return Ok(0);
	} else if pnl_to_settle_with_user == 0 {
		msg!(
			"Pnl Pool cannot currently settle with user for market {}",
			market.market_index
		);
		return Ok(0);
	}

	let user_spot_position = user.get_quote_spot_position_mut();

	transfer_spot_balances(
		pnl_to_settle_with_user,
		bank,
		&mut market.pnl_pool,
		user_spot_position
	)?;

	Ok(pnl_to_settle_with_user)
}

// TODO: end review

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
