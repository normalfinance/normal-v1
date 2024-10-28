use anchor_lang::prelude::*;
use borsh::{ BorshDeserialize, BorshSerialize };
use solana_program::msg;

use crate::controller;
use crate::controller::amm::SwapDirection;
use crate::error::{ NormalResult, ErrorCode };
use crate::math::casting::Cast;
use crate::math::constants::{ MAX_BASE_ASSET_AMOUNT_WITH_AMM, PERP_DECIMALS };
use crate::math::orders::{
	calculate_quote_asset_amount_for_maker_order,
	get_position_delta_for_fill,
	is_multiple_of_step_size,
};
use crate::math::position::{
	get_new_position_amounts,
	get_position_update_type,
	PositionUpdateType,
};
use crate::math::safe_math::SafeMath;
use crate::math_error;
use crate::safe_increment;
use crate::state::market::Market;
use crate::state::amm::AMMLiquiditySplit;
use crate::state::user::{ Position, Positions, User };
use crate::validate;

// #[cfg(test)]
// mod tests;

pub fn add_new_position(
	user_positions: &mut Positions,
	market_index: u16
) -> NormalResult<usize> {
	let new_position_index = user_positions
		.iter()
		.position(|market_position| market_position.is_available())
		.ok_or(ErrorCode::MaxNumberOfPositions)?;

	let new_market_position = Position {
		market_index,
		..Position::default()
	};

	user_positions[new_position_index] = new_market_position;

	Ok(new_position_index)
}

pub fn get_position_index(
	user_positions: &Positions,
	market_index: u16
) -> NormalResult<usize> {
	let position_index = user_positions
		.iter()
		.position(|market_position| market_position.is_for(market_index));

	match position_index {
		Some(position_index) => Ok(position_index),
		None => Err(ErrorCode::UserHasNoPositionInMarket),
	}
}

#[derive(Default, PartialEq, Debug)]
pub struct PositionDelta {
	pub quote_asset_amount: i64,
	pub base_asset_amount: i64,
	pub remainder_base_asset_amount: Option<i64>,
}

impl PositionDelta {
	pub fn get_delta_base_with_remainder_abs(&self) -> NormalResult<i128> {
		let delta_base_i128 = if
			let Some(remainder_base_asset_amount) = self.remainder_base_asset_amount
		{
			self.base_asset_amount
				.safe_add(remainder_base_asset_amount.cast()?)?
				.abs()
				.cast::<i128>()?
		} else {
			self.base_asset_amount.abs().cast::<i128>()?
		};
		Ok(delta_base_i128)
	}
}

pub fn update_position_and_market(
	position: &mut Position,
	market: &mut Market,
	delta: &PositionDelta
) -> NormalResult<i64> {
	if
		delta.base_asset_amount == 0 &&
		delta.remainder_base_asset_amount.unwrap_or(0) == 0
	{
		update_quote_asset_amount(position, market, delta.quote_asset_amount)?;
		return Ok(delta.quote_asset_amount);
	}

	let update_type = get_position_update_type(position, delta)?;

	// Update User
	let (
		new_base_asset_amount,
		new_settled_base_asset_amount,
		new_remainder_base_asset_amount,
	) = get_new_position_amounts(position, delta, market)?;

	market.update_market_with_counterparty(delta, new_settled_base_asset_amount)?;

	// Update Market open interest
	if let PositionUpdateType::Open = update_type {
		// If the user is new
		if position.base_asset_amount() == 0 {
			market.number_of_users = market.number_of_users.safe_add(1)?;
		}

		market.number_of_users_with_base =
			market.number_of_users_with_base.safe_add(1)?;
	} else if let PositionUpdateType::Close = update_type {
		if new_base_asset_amount == 0 {
			market.number_of_users = market.number_of_users.safe_sub(1)?;
		}

		market.number_of_users_with_base =
			market.number_of_users_with_base.safe_sub(1)?;
	}

	market.amm.quote_asset_amount = market.amm.quote_asset_amount.safe_add(
		delta.quote_asset_amount.cast()?
	)?;

	match update_type {
		PositionUpdateType::Open | PositionUpdateType::Increase => {
			if new_base_asset_amount > 0 {
				market.amm.base_asset_amount_long =
					market.amm.base_asset_amount_long.safe_add(
						new_settled_base_asset_amount.cast()?
					)?;
			}
		}
		PositionUpdateType::Reduce | PositionUpdateType::Close => {
			if position.base_asset_amount() > 0 {
				market.amm.base_asset_amount_long =
					market.amm.base_asset_amount_long.safe_add(
						new_settled_base_asset_amount.cast()?
					)?;
			}
		}
	}

	let new_position_base_with_remainder = new_base_asset_amount.safe_add(
		new_remainder_base_asset_amount
	)?;

	validate!(
		is_multiple_of_step_size(
			position.base_asset_amount().unsigned_abs(),
			market.amm.order_step_size
		)?,
		ErrorCode::InvalidPositionDetected,
		"update_position_and_market left invalid position before {} after {}",
		position.base_asset_amount(),
		new_base_asset_amount
	)?;

	position.remainder_base_asset_amount =
		new_remainder_base_asset_amount.cast::<i32>()?;

	Ok(pnl)
}

pub fn update_lp_market_position(
	market: &mut Market,
	delta: &PositionDelta,
	fee_to_market: i128,
	liquidity_split: AMMLiquiditySplit
) -> NormalResult<i128> {
	if
		market.amm.user_lp_shares == 0 ||
		liquidity_split == AMMLiquiditySplit::ProtocolOwned
	{
		return Ok(0); // no need to split with LP
	}

	let base_unit: i128 = market.amm.get_per_lp_base_unit()?;

	let (per_lp_delta_base, per_lp_delta_quote, per_lp_fee) =
		market.amm.calculate_per_lp_delta(
			delta,
			fee_to_market,
			liquidity_split,
			base_unit
		)?;

	market.amm.base_asset_amount_per_lp =
		market.amm.base_asset_amount_per_lp.safe_add(-per_lp_delta_base)?;

	market.amm.quote_asset_amount_per_lp =
		market.amm.quote_asset_amount_per_lp.safe_add(-per_lp_delta_quote)?;

	// track total fee earned by lps (to attribute breakdown of IL)
	market.amm.total_fee_earned_per_lp =
		market.amm.total_fee_earned_per_lp.saturating_add(per_lp_fee.cast()?);

	// update per lp position
	market.amm.quote_asset_amount_per_lp =
		market.amm.quote_asset_amount_per_lp.safe_add(per_lp_fee)?;

	let lp_delta_base = market.amm.calculate_lp_base_delta(
		per_lp_delta_base,
		base_unit
	)?;
	let lp_delta_quote = market.amm.calculate_lp_base_delta(
		per_lp_delta_quote,
		base_unit
	)?;

	market.amm.base_asset_amount_with_amm =
		market.amm.base_asset_amount_with_amm.safe_sub(lp_delta_base)?;

	market.amm.base_asset_amount_with_unsettled_lp =
		market.amm.base_asset_amount_with_unsettled_lp.safe_add(lp_delta_base)?;

	market.amm.quote_asset_amount_with_unsettled_lp =
		market.amm.quote_asset_amount_with_unsettled_lp.safe_add(
			lp_delta_quote.cast()?
		)?;

	Ok(lp_delta_base)
}

pub fn update_position_with_base_asset_amount(
	base_asset_amount: u64,
	side: OrderSide,
	market: &mut Market,
	user: &mut User,
	position_index: usize,
	fill_price: Option<u64>
) -> NormalResult<(u64, i64, i64)> {
	let swap_direction = match side {
		OrderSide::Buy => SwapDirection::Remove,
		OrderSide::Sell => SwapDirection::Add,
	};

	let (quote_asset_swapped, quote_asset_amount_surplus) =
		controller::amm::swap_base_asset(
			market,
			base_asset_amount,
			swap_direction
		)?;

	let (quote_asset_amount, quote_asset_amount_surplus) = match fill_price {
		Some(fill_price) =>
			calculate_quote_asset_amount_surplus(
				side,
				quote_asset_swapped,
				base_asset_amount,
				fill_price
			)?,
		None => (quote_asset_swapped, quote_asset_amount_surplus),
	};

	let position_delta = get_position_delta_for_fill(
		base_asset_amount,
		quote_asset_amount,
		side
	)?;

	let pnl = update_position_and_market(
		&mut user.positions[position_index],
		market,
		&position_delta
	)?;

	market.amm.base_asset_amount_with_amm =
		market.amm.base_asset_amount_with_amm.safe_add(
			position_delta.base_asset_amount.cast()?
		)?;

	validate!(
		market.amm.base_asset_amount_with_amm.unsigned_abs() <=
			MAX_BASE_ASSET_AMOUNT_WITH_AMM,
		ErrorCode::InvalidAmmDetected,
		"market.amm.base_asset_amount_with_amm={} cannot exceed MAX_BASE_ASSET_AMOUNT_WITH_AMM",
		market.amm.base_asset_amount_with_amm
	)?;

	controller::amm::update_spread_reserves(market)?;

	Ok((quote_asset_amount, quote_asset_amount_surplus, pnl))
}

fn calculate_quote_asset_amount_surplus(
	position_side: OrderSide,
	quote_asset_swapped: u64,
	base_asset_amount: u64,
	fill_price: u64
) -> NormalResult<(u64, i64)> {
	let quote_asset_amount = calculate_quote_asset_amount_for_maker_order(
		base_asset_amount,
		fill_price,
		PERP_DECIMALS,
		position_side
	)?;

	let quote_asset_amount_surplus = match position_side {
		OrderSide::Buy =>
			quote_asset_amount.cast::<i64>()?.safe_sub(quote_asset_swapped.cast()?)?,
		OrderSide::Sell =>
			quote_asset_swapped.cast::<i64>()?.safe_sub(quote_asset_amount.cast()?)?,
	};

	Ok((quote_asset_amount, quote_asset_amount_surplus))
}

pub fn update_quote_asset_amount(
	position: &mut Position,
	market: &mut Market,
	delta: i64
) -> NormalResult<()> {
	if delta == 0 {
		return Ok(());
	}

	if
		position.base_asset_amount() == 0 &&
		position.remainder_base_asset_amount == 0
	{
		market.number_of_users = market.number_of_users.safe_add(1)?;
	}

	market.amm.quote_asset_amount = market.amm.quote_asset_amount.safe_add(
		delta.cast()?
	)?;

	if
		position.base_asset_amount() == 0 &&
		position.remainder_base_asset_amount == 0
	{
		market.number_of_users = market.number_of_users.saturating_sub(1);
	}

	Ok(())
}

pub fn increase_open_bids_and_asks(
	position: &mut Position,
	side: &OrderSide,
	base_asset_amount_unfilled: u64
) -> NormalResult {
	match side {
		OrderSide::Buy => {
			position.open_bids = position.open_bids.safe_add(
				base_asset_amount_unfilled.cast()?
			)?;
		}
		OrderSide::Sell => {
			position.open_asks = position.open_asks.safe_sub(
				base_asset_amount_unfilled.cast()?
			)?;
		}
	}

	Ok(())
}

pub fn decrease_open_bids_and_asks(
	position: &mut Position,
	side: &OrderSide,
	base_asset_amount_unfilled: u64
) -> NormalResult {
	match side {
		OrderSide::Buy => {
			position.open_bids = position.open_bids.safe_sub(
				base_asset_amount_unfilled.cast()?
			)?;
		}
		OrderSide::Sell => {
			position.open_asks = position.open_asks.safe_add(
				base_asset_amount_unfilled.cast()?
			)?;
		}
	}

	Ok(())
}
