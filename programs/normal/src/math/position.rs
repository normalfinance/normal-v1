use solana_program::msg;

use crate::controller::amm::SwapDirection;
use crate::controller::position::PositionDelta;
use crate::error::{ NormalResult, ErrorCode };
use crate::math::amm;
use crate::math::amm::calculate_quote_asset_amount_swapped;
use crate::math::casting::Cast;
use crate::math::constants::{
	AMM_RESERVE_PRECISION_I128,
	PRICE_TIMES_AMM_TO_QUOTE_PRECISION_RATIO,
	PRICE_TIMES_AMM_TO_QUOTE_PRECISION_RATIO_I128,
};
use crate::math::helpers::get_proportion_u128;
use crate::math::pnl::calculate_pnl;
use crate::math::safe_math::SafeMath;

use crate::state::market::{ AssetType, Market };
use crate::state::amm::AMM;
use crate::state::user::Position;
use crate::{ validate, BASE_PRECISION };

pub fn calculate_base_asset_value_and_pnl(
	base_asset_amount: i128,
	quote_asset_amount: u128,
	amm: &AMM
) -> NormalResult<(u128, i128)> {
	if base_asset_amount == 0 {
		return Ok((0, 0));
	}
	let swap_direction = swap_direction_to_close_position(base_asset_amount);
	let base_asset_value = calculate_base_asset_value(base_asset_amount, amm)?;
	let pnl = calculate_pnl(
		base_asset_value,
		quote_asset_amount,
		swap_direction
	)?;

	Ok((base_asset_value, pnl))
}

pub fn calculate_base_asset_value(
	base_asset_amount: i128,
	amm: &AMM
) -> NormalResult<u128> {
	if base_asset_amount == 0 {
		return Ok(0);
	}

	let swap_direction = swap_direction_to_close_position(base_asset_amount);

	let (base_asset_reserve, quote_asset_reserve) = (
		amm.base_asset_reserve,
		amm.quote_asset_reserve,
	);

	let amm_lp_shares = amm.sqrt_k.safe_sub(amm.user_lp_shares)?;

	let base_asset_reserve_proportion = get_proportion_u128(
		base_asset_reserve,
		amm_lp_shares,
		amm.sqrt_k
	)?;

	let quote_asset_reserve_proportion = get_proportion_u128(
		quote_asset_reserve,
		amm_lp_shares,
		amm.sqrt_k
	)?;

	let (new_quote_asset_reserve, _new_base_asset_reserve) =
		amm::calculate_swap_output(
			base_asset_amount.unsigned_abs(),
			base_asset_reserve_proportion,
			swap_direction,
			amm_lp_shares
		)?;

	let base_asset_value = calculate_quote_asset_amount_swapped(
		quote_asset_reserve_proportion,
		new_quote_asset_reserve,
		swap_direction,
		amm.peg_multiplier
	)?;

	Ok(base_asset_value)
}

pub fn calculate_base_asset_value_with_oracle_price(
	base_asset_amount: i128,
	oracle_price: i64
) -> NormalResult<u128> {
	if base_asset_amount == 0 {
		return Ok(0);
	}

	let oracle_price = if oracle_price > 0 {
		oracle_price.unsigned_abs()
	} else {
		0
	};

	base_asset_amount
		.unsigned_abs()
		.safe_mul(oracle_price.cast()?)?
		.safe_div(PRICE_TIMES_AMM_TO_QUOTE_PRECISION_RATIO)
}

pub fn calculate_base_asset_value_with_expiry_price(
	market_position: &Position,
	expiry_price: i64
) -> NormalResult<i64> {
	if market_position.base_asset_amount() == 0 {
		return Ok(0);
	}

	market_position
		.base_asset_amount()
		.cast::<i128>()?
		.safe_mul(expiry_price.cast()?)?
		.safe_div(PRICE_TIMES_AMM_TO_QUOTE_PRECISION_RATIO_I128)?
		.cast::<i64>()
}

pub fn swap_direction_to_close_position(
	base_asset_amount: i128
) -> SwapDirection {
	if base_asset_amount >= 0 {
		SwapDirection::Add
	} else {
		SwapDirection::Remove
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PositionUpdateType {
	Open,
	Increase,
	Reduce,
	Close,
}
pub fn get_position_update_type(
	position: &Position,
	delta: &PositionDelta
) -> NormalResult<PositionUpdateType> {
	if
		position.base_asset_amount() == 0 &&
		position.remainder_base_asset_amount == 0
	{
		return Ok(PositionUpdateType::Open);
	}

	let position_base_with_remainder = if
		position.remainder_base_asset_amount != 0
	{
		position
			.base_asset_amount()
			.safe_add(position.remainder_base_asset_amount.cast::<i64>()?)?
	} else {
		position.base_asset_amount()
	};

	let delta_base_with_remainder = if
		let Some(remainder_base_asset_amount) = delta.remainder_base_asset_amount
	{
		delta.base_asset_amount.safe_add(remainder_base_asset_amount.cast()?)?
	} else {
		delta.base_asset_amount
	};

	if
		position_base_with_remainder.signum() == delta_base_with_remainder.signum()
	{
		Ok(PositionUpdateType::Increase)
	} else if
		position_base_with_remainder.abs() > delta_base_with_remainder.abs()
	{
		Ok(PositionUpdateType::Reduce)
	} else if
		position_base_with_remainder.abs() == delta_base_with_remainder.abs()
	{
		Ok(PositionUpdateType::Close)
	}
}

pub fn get_new_position_amounts(
	position: &Position,
	delta: &PositionDelta,
	market: &PerpMarket
) -> NormalResult<(i64, i64, i64)> {
	let mut new_base_asset_amount = position.base_asset_amount().safe_add(
		delta.base_asset_amount
	)?;

	let mut new_remainder_base_asset_amount = position.remainder_base_asset_amount
		.cast::<i64>()?
		.safe_add(delta.remainder_base_asset_amount.unwrap_or(0).cast::<i64>()?)?;
	let mut new_settled_base_asset_amount = delta.base_asset_amount;

	if delta.remainder_base_asset_amount.is_some() {
		if
			new_remainder_base_asset_amount.unsigned_abs() >=
			market.amm.order_step_size
		{
			let (
				standardized_remainder_base_asset_amount,
				remainder_base_asset_amount,
			) = crate::math::orders::standardize_base_asset_amount_with_remainder_i128(
				new_remainder_base_asset_amount.cast()?,
				market.amm.order_step_size.cast()?
			)?;

			new_base_asset_amount = new_base_asset_amount.safe_add(
				standardized_remainder_base_asset_amount.cast()?
			)?;

			new_settled_base_asset_amount = new_settled_base_asset_amount.safe_add(
				standardized_remainder_base_asset_amount.cast()?
			)?;

			new_remainder_base_asset_amount = remainder_base_asset_amount.cast()?;
		} else {
			new_remainder_base_asset_amount = new_remainder_base_asset_amount.cast()?;
		}

		validate!(
			new_remainder_base_asset_amount.abs() <= (i32::MAX as i64),
			ErrorCode::InvalidPositionDelta,
			"new_remainder_base_asset_amount={} > i32 max",
			new_remainder_base_asset_amount
		)?;
	}

	Ok((
		new_base_asset_amount,
		new_settled_base_asset_amount,
		new_remainder_base_asset_amount,
	))
}
