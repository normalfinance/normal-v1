use anchor_lang::prelude::{ AccountLoader, * };

use crate::state::{
	amm::{ AMMRewardInfo, AMM, NUM_REWARDS },
	liquidity_position::{ LiquidityPosition, LiquidityPositionUpdate },
	tick::{ TickArray, TickUpdate },
};

#[derive(Debug)]
pub struct ModifyLiquidityUpdate {
	pub amm_liquidity: u128,
	pub tick_lower_update: TickUpdate,
	pub tick_upper_update: TickUpdate,
	pub reward_infos: [AMMRewardInfo; NUM_REWARDS],
	pub position_update: LiquidityPositionUpdate,
}

// Calculates state after modifying liquidity by the liquidity_delta for the given positon.
// Fee and reward growths will also be calculated by this function.
// To trigger only calculation of fee and reward growths, use calculate_fee_and_reward_growths.
pub fn calculate_modify_liquidity<'info>(
	amm: &AMM,
	position: &LiquidityPosition,
	tick_array_lower: &AccountLoader<'info, TickArray>,
	tick_array_upper: &AccountLoader<'info, TickArray>,
	liquidity_delta: i128,
	timestamp: u64
) -> Result<ModifyLiquidityUpdate> {
	let tick_array_lower = tick_array_lower.load()?;
	let tick_lower = tick_array_lower.get_tick(
		position.tick_lower_index,
		amm.tick_spacing
	)?;

	let tick_array_upper = tick_array_upper.load()?;
	let tick_upper = tick_array_upper.get_tick(
		position.tick_upper_index,
		amm.tick_spacing
	)?;

	_calculate_modify_liquidity(
		amm,
		position,
		tick_lower,
		tick_upper,
		position.tick_lower_index,
		position.tick_upper_index,
		liquidity_delta,
		timestamp
	)
}

pub fn calculate_fee_and_reward_growths<'info>(
	amm: &AMM,
	position: &Position,
	tick_array_lower: &AccountLoader<'info, TickArray>,
	tick_array_upper: &AccountLoader<'info, TickArray>,
	timestamp: u64
) -> Result<(LiquidityPositionUpdate, [AMMRewardInfo; NUM_REWARDS])> {
	let tick_array_lower = tick_array_lower.load()?;
	let tick_lower = tick_array_lower.get_tick(
		position.tick_lower_index,
		amm.tick_spacing
	)?;

	let tick_array_upper = tick_array_upper.load()?;
	let tick_upper = tick_array_upper.get_tick(
		position.tick_upper_index,
		amm.tick_spacing
	)?;

	// Pass in a liquidity_delta value of 0 to trigger only calculations for fee and reward growths.
	// Calculating fees and rewards for positions with zero liquidity will result in an error.
	let update = _calculate_modify_liquidity(
		amm,
		position,
		tick_lower,
		tick_upper,
		position.tick_lower_index,
		position.tick_upper_index,
		0,
		timestamp
	)?;
	Ok((update.position_update, update.reward_infos))
}

// Calculates the state changes after modifying liquidity of a amm position.
#[allow(clippy::too_many_arguments)]
fn _calculate_modify_liquidity(
	amm: &AMM,
	position: &LiquidityPosition,
	tick_lower: &Tick,
	tick_upper: &Tick,
	tick_lower_index: i32,
	tick_upper_index: i32,
	liquidity_delta: i128,
	timestamp: u64
) -> Result<ModifyLiquidityUpdate> {
	// Disallow only updating position fee and reward growth when position has zero liquidity
	if liquidity_delta == 0 && position.liquidity == 0 {
		return Err(ErrorCode::LiquidityZero.into());
	}

	let next_reward_infos = controller::amm::next_amm_reward_infos(
		amm,
		timestamp
	)?;

	let next_global_liquidity = controller::amm::next_amm_liquidity(
		amm,
		position.tick_upper_index,
		position.tick_lower_index,
		liquidity_delta
	)?;

	let tick_lower_update = controller::tick::next_tick_modify_liquidity_update(
		tick_lower,
		tick_lower_index,
		amm.tick_current_index,
		amm.fee_growth_global_a,
		amm.fee_growth_global_b,
		&next_reward_infos,
		liquidity_delta,
		false
	)?;

	let tick_upper_update = controller::tick::next_tick_modify_liquidity_update(
		tick_upper,
		tick_upper_index,
		amm.tick_current_index,
		amm.fee_growth_global_a,
		amm.fee_growth_global_b,
		&next_reward_infos,
		liquidity_delta,
		true
	)?;

	let (fee_growth_inside_synthetic, fee_growth_inside_quote) =
		controller::tick::next_fee_growths_inside(
			amm.tick_current_index,
			tick_lower,
			tick_lower_index,
			tick_upper,
			tick_upper_index,
			amm.fee_growth_global_a,
			amm.fee_growth_global_b
		);

	let reward_growths_inside = controller::tick::next_reward_growths_inside(
		amm.tick_current_index,
		tick_lower,
		tick_lower_index,
		tick_upper,
		tick_upper_index,
		&next_reward_infos
	);

	let position_update =
		controller::position::next_position_modify_liquidity_update(
			position,
			liquidity_delta,
			fee_growth_inside_synthetic,
			fee_growth_inside_quote,
			&reward_growths_inside
		)?;

	Ok(ModifyLiquidityUpdate {
		amm_liquidity: next_global_liquidity,
		reward_infos: next_reward_infos,
		position_update,
		tick_lower_update,
		tick_upper_update,
	})
}

pub fn calculate_liquidity_token_deltas(
	current_tick_index: i32,
	sqrt_price: u128,
	position: &LiquidityPosition,
	liquidity_delta: i128
) -> Result<(u64, u64)> {
	if liquidity_delta == 0 {
		return Err(ErrorCode::LiquidityZero.into());
	}

	let mut delta_a: u64 = 0;
	let mut delta_b: u64 = 0;

	let liquidity: u128 = liquidity_delta.unsigned_abs();
	let round_up = liquidity_delta > 0;

	let lower_price = math::amm::sqrt_price_from_tick_index(
		position.tick_lower_index
	);
	let upper_price = math::amm::sqrt_price_from_tick_index(
		position.tick_upper_index
	);

	if current_tick_index < position.tick_lower_index {
		// current tick below position
		delta_a = math::amm::get_amount_delta_a(
			lower_price,
			upper_price,
			liquidity,
			round_up
		)?;
	} else if current_tick_index < position.tick_upper_index {
		// current tick inside position
		delta_a = math::amm::get_amount_delta_a(
			sqrt_price,
			upper_price,
			liquidity,
			round_up
		)?;
		delta_b = math::amm::get_amount_delta_b(
			lower_price,
			sqrt_price,
			liquidity,
			round_up
		)?;
	} else {
		// current tick above position
		delta_b = math::amm::get_amount_delta_b(
			lower_price,
			upper_price,
			liquidity,
			round_up
		)?;
	}

	Ok((delta_a, delta_b))
}

pub fn sync_modify_liquidity_values<'info>(
	amm: &mut AMM,
	position: &mut LiquidityPosition,
	tick_array_lower: &AccountLoader<'info, TickArray>,
	tick_array_upper: &AccountLoader<'info, TickArray>,
	modify_liquidity_update: ModifyLiquidityUpdate,
	reward_last_updated_timestamp: u64
) -> Result<()> {
	position.update(&modify_liquidity_update.position_update);

	tick_array_lower
		.load_mut()?
		.update_tick(
			position.tick_lower_index,
			amm.tick_spacing,
			&modify_liquidity_update.tick_lower_update
		)?;

	tick_array_upper
		.load_mut()?
		.update_tick(
			position.tick_upper_index,
			amm.tick_spacing,
			&modify_liquidity_update.tick_upper_update
		)?;

	amm.update_rewards_and_liquidity(
		modify_liquidity_update.reward_infos,
		modify_liquidity_update.amm_liquidity,
		reward_last_updated_timestamp
	);

	Ok(())
}
