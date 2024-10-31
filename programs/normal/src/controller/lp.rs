use super::{
	position_manager::next_position_modify_liquidity_update,
	tick_manager::{
		next_fee_growths_inside,
		next_reward_growths_inside,
		next_tick_modify_liquidity_update,
	},
	amm_manager::{ next_amm_liquidity, next_amm_reward_infos },
};
use crate::{
	errors::ErrorCode,
	math,
	state::{ Position, PositionUpdate, NUM_REWARDS, * },
};
use crate::math::lp::{
		get_amount_delta_a,
		get_amount_delta_b,
		sqrt_price_from_tick_index,
		add_liquidity_delta,
		checked_mul_shift_right,
};
use amm::AMM;
use anchor_lang::prelude::{ AccountLoader, * };

// From amm_manager.rs

// Calculates the next global reward growth variables based on the given timestamp.
// The provided timestamp must be greater than or equal to the last updated timestamp.
pub fn next_amm_reward_infos(
	amm: &AMM,
	next_timestamp: u64
) -> Result<[AMMRewardInfo; NUM_REWARDS], ErrorCode> {
	let curr_timestamp = amm.reward_last_updated_timestamp;
	if next_timestamp < curr_timestamp {
		return Err(ErrorCode::InvalidTimestamp);
	}

	// No-op if no liquidity or no change in timestamp
	if amm.liquidity == 0 || next_timestamp == curr_timestamp {
		return Ok(amm.reward_infos);
	}

	// Calculate new global reward growth
	let mut next_reward_infos = amm.reward_infos;
	let time_delta = u128::from(next_timestamp - curr_timestamp);
	for reward_info in next_reward_infos.iter_mut() {
		if !reward_info.initialized() {
			continue;
		}

		// Calculate the new reward growth delta.
		// If the calculation overflows, set the delta value to zero.
		// This will halt reward distributions for this reward.
		let reward_growth_delta = checked_mul_div(
			time_delta,
			reward_info.emissions_per_second_x64,
			amm.liquidity
		).unwrap_or(0);

		// Add the reward growth delta to the global reward growth.
		let curr_growth_global = reward_info.growth_global_x64;
		reward_info.growth_global_x64 =
			curr_growth_global.wrapping_add(reward_growth_delta);
	}

	Ok(next_reward_infos)
}

// Calculates the next global liquidity for a amm depending on its position relative
// to the lower and upper tick indexes and the liquidity_delta.
pub fn next_amm_liquidity(
	amm: &AMM,
	tick_upper_index: i32,
	tick_lower_index: i32,
	liquidity_delta: i128
) -> Result<u128, ErrorCode> {
	if
		amm.tick_current_index < tick_upper_index &&
		amm.tick_current_index >= tick_lower_index
	{
		add_liquidity_delta(amm.liquidity, liquidity_delta)
	} else {
		Ok(amm.liquidity)
	}
}

// From liquidity_manager.rs

#[derive(Debug)]
pub struct ModifyLiquidityUpdate {
	pub amm_liquidity: u128,
	pub tick_lower_update: TickUpdate,
	pub tick_upper_update: TickUpdate,
	pub reward_infos: [AMMRewardInfo; NUM_REWARDS],
	pub position_update: PositionUpdate,
}

// Calculates state after modifying liquidity by the liquidity_delta for the given positon.
// Fee and reward growths will also be calculated by this function.
// To trigger only calculation of fee and reward growths, use calculate_fee_and_reward_growths.
pub fn calculate_modify_liquidity<'info>(
	amm: &AMM,
	position: &Position,
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
) -> Result<(PositionUpdate, [AMMRewardInfo; NUM_REWARDS])> {
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
	position: &Position,
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

	let next_reward_infos = next_amm_reward_infos(amm, timestamp)?;

	let next_global_liquidity = next_amm_liquidity(
		amm,
		position.tick_upper_index,
		position.tick_lower_index,
		liquidity_delta
	)?;

	let tick_lower_update = next_tick_modify_liquidity_update(
		tick_lower,
		tick_lower_index,
		amm.tick_current_index,
		amm.fee_growth_global_a,
		amm.fee_growth_global_b,
		&next_reward_infos,
		liquidity_delta,
		false
	)?;

	let tick_upper_update = next_tick_modify_liquidity_update(
		tick_upper,
		tick_upper_index,
		amm.tick_current_index,
		amm.fee_growth_global_a,
		amm.fee_growth_global_b,
		&next_reward_infos,
		liquidity_delta,
		true
	)?;

	let (fee_growth_inside_a, fee_growth_inside_b) = next_fee_growths_inside(
		amm.tick_current_index,
		tick_lower,
		tick_lower_index,
		tick_upper,
		tick_upper_index,
		amm.fee_growth_global_a,
		amm.fee_growth_global_b
	);

	let reward_growths_inside = next_reward_growths_inside(
		amm.tick_current_index,
		tick_lower,
		tick_lower_index,
		tick_upper,
		tick_upper_index,
		&next_reward_infos
	);

	let position_update = next_position_modify_liquidity_update(
		position,
		liquidity_delta,
		fee_growth_inside_a,
		fee_growth_inside_b,
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
	position: &Position,
	liquidity_delta: i128
) -> Result<(u64, u64)> {
	if liquidity_delta == 0 {
		return Err(ErrorCode::LiquidityZero.into());
	}

	let mut delta_a: u64 = 0;
	let mut delta_b: u64 = 0;

	let liquidity: u128 = liquidity_delta.unsigned_abs();
	let round_up = liquidity_delta > 0;

	let lower_price = sqrt_price_from_tick_index(position.tick_lower_index);
	let upper_price = sqrt_price_from_tick_index(position.tick_upper_index);

	if current_tick_index < position.tick_lower_index {
		// current tick below position
		delta_a = get_amount_delta_a(
			lower_price,
			upper_price,
			liquidity,
			round_up
		)?;
	} else if current_tick_index < position.tick_upper_index {
		// current tick inside position
		delta_a = get_amount_delta_a(sqrt_price, upper_price, liquidity, round_up)?;
		delta_b = get_amount_delta_b(lower_price, sqrt_price, liquidity, round_up)?;
	} else {
		// current tick above position
		delta_b = get_amount_delta_b(
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
	position: &mut Position,
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

pub fn next_position_modify_liquidity_update(
	position: &Position,
	liquidity_delta: i128,
	fee_growth_inside_a: u128,
	fee_growth_inside_b: u128,
	reward_growths_inside: &[u128; NUM_REWARDS]
) -> Result<PositionUpdate, ErrorCode> {
	let mut update = PositionUpdate::default();

	// Calculate fee deltas.
	// If fee deltas overflow, default to a zero value. This means the position loses
	// all fees earned since the last time the position was modified or fees collected.
	let growth_delta_a = fee_growth_inside_a.wrapping_sub(
		position.fee_growth_checkpoint_a
	);
	let fee_delta_a = checked_mul_shift_right(
		position.liquidity,
		growth_delta_a
	).unwrap_or(0);

	let growth_delta_b = fee_growth_inside_b.wrapping_sub(
		position.fee_growth_checkpoint_b
	);
	let fee_delta_b = checked_mul_shift_right(
		position.liquidity,
		growth_delta_b
	).unwrap_or(0);

	update.fee_growth_checkpoint_a = fee_growth_inside_a;
	update.fee_growth_checkpoint_b = fee_growth_inside_b;

	// Overflows allowed. Must collect fees owed before overflow.
	update.fee_owed_a = position.fee_owed_a.wrapping_add(fee_delta_a);
	update.fee_owed_b = position.fee_owed_b.wrapping_add(fee_delta_b);

	for (i, update) in update.reward_infos.iter_mut().enumerate() {
		let reward_growth_inside = reward_growths_inside[i];
		let curr_reward_info = position.reward_infos[i];

		// Calculate reward delta.
		// If reward delta overflows, default to a zero value. This means the position loses all
		// rewards earned since the last time the position was modified or rewards were collected.
		let reward_growth_delta = reward_growth_inside.wrapping_sub(
			curr_reward_info.growth_inside_checkpoint
		);
		let amount_owed_delta = checked_mul_shift_right(
			position.liquidity,
			reward_growth_delta
		).unwrap_or(0);

		update.growth_inside_checkpoint = reward_growth_inside;

		// Overflows allowed. Must collect rewards owed before overflow.
		update.amount_owed =
			curr_reward_info.amount_owed.wrapping_add(amount_owed_delta);
	}

	update.liquidity = add_liquidity_delta(position.liquidity, liquidity_delta)?;

	Ok(update)
}

// From tick_manager.rs

pub fn next_tick_cross_update(
	tick: &Tick,
	fee_growth_global_a: u128,
	fee_growth_global_b: u128,
	reward_infos: &[AMMRewardInfo; NUM_REWARDS]
) -> Result<TickUpdate, ErrorCode> {
	let mut update = TickUpdate::from(tick);

	update.fee_growth_outside_a = fee_growth_global_a.wrapping_sub(
		tick.fee_growth_outside_a
	);
	update.fee_growth_outside_b = fee_growth_global_b.wrapping_sub(
		tick.fee_growth_outside_b
	);

	for (i, reward_info) in reward_infos.iter().enumerate() {
		if !reward_info.initialized() {
			continue;
		}

		update.reward_growths_outside[i] =
			reward_info.growth_global_x64.wrapping_sub(
				tick.reward_growths_outside[i]
			);
	}
	Ok(update)
}

#[allow(clippy::too_many_arguments)]
pub fn next_tick_modify_liquidity_update(
	tick: &Tick,
	tick_index: i32,
	tick_current_index: i32,
	fee_growth_global_a: u128,
	fee_growth_global_b: u128,
	reward_infos: &[AMMRewardInfo; NUM_REWARDS],
	liquidity_delta: i128,
	is_upper_tick: bool
) -> Result<TickUpdate, ErrorCode> {
	// noop if there is no change in liquidity
	if liquidity_delta == 0 {
		return Ok(TickUpdate::from(tick));
	}

	let liquidity_gross = add_liquidity_delta(
		tick.liquidity_gross,
		liquidity_delta
	)?;

	// Update to an uninitialized tick if remaining liquidity is being removed
	if liquidity_gross == 0 {
		return Ok(TickUpdate::default());
	}

	let (fee_growth_outside_a, fee_growth_outside_b, reward_growths_outside) = if
		tick.liquidity_gross == 0
	{
		// By convention, assume all prior growth happened below the tick
		if tick_current_index >= tick_index {
			(
				fee_growth_global_a,
				fee_growth_global_b,
				AMMRewardInfo::to_reward_growths(reward_infos),
			)
		} else {
			(0, 0, [0; NUM_REWARDS])
		}
	} else {
		(
			tick.fee_growth_outside_a,
			tick.fee_growth_outside_b,
			tick.reward_growths_outside,
		)
	};

	let liquidity_net = if is_upper_tick {
		tick.liquidity_net
			.checked_sub(liquidity_delta)
			.ok_or(ErrorCode::LiquidityNetError)?
	} else {
		tick.liquidity_net
			.checked_add(liquidity_delta)
			.ok_or(ErrorCode::LiquidityNetError)?
	};

	Ok(TickUpdate {
		initialized: true,
		liquidity_net,
		liquidity_gross,
		fee_growth_outside_a,
		fee_growth_outside_b,
		reward_growths_outside,
	})
}

// Calculates the fee growths inside of tick_lower and tick_upper based on their
// index relative to tick_current_index.
pub fn next_fee_growths_inside(
	tick_current_index: i32,
	tick_lower: &Tick,
	tick_lower_index: i32,
	tick_upper: &Tick,
	tick_upper_index: i32,
	fee_growth_global_a: u128,
	fee_growth_global_b: u128
) -> (u128, u128) {
	// By convention, when initializing a tick, all fees have been earned below the tick.
	let (fee_growth_below_a, fee_growth_below_b) = if !tick_lower.initialized {
		(fee_growth_global_a, fee_growth_global_b)
	} else if tick_current_index < tick_lower_index {
		(
			fee_growth_global_a.wrapping_sub(tick_lower.fee_growth_outside_a),
			fee_growth_global_b.wrapping_sub(tick_lower.fee_growth_outside_b),
		)
	} else {
		(tick_lower.fee_growth_outside_a, tick_lower.fee_growth_outside_b)
	};

	// By convention, when initializing a tick, no fees have been earned above the tick.
	let (fee_growth_above_a, fee_growth_above_b) = if !tick_upper.initialized {
		(0, 0)
	} else if tick_current_index < tick_upper_index {
		(tick_upper.fee_growth_outside_a, tick_upper.fee_growth_outside_b)
	} else {
		(
			fee_growth_global_a.wrapping_sub(tick_upper.fee_growth_outside_a),
			fee_growth_global_b.wrapping_sub(tick_upper.fee_growth_outside_b),
		)
	};

	(
		fee_growth_global_a
			.wrapping_sub(fee_growth_below_a)
			.wrapping_sub(fee_growth_above_a),
		fee_growth_global_b
			.wrapping_sub(fee_growth_below_b)
			.wrapping_sub(fee_growth_above_b),
	)
}

// Calculates the reward growths inside of tick_lower and tick_upper based on their positions
// relative to tick_current_index. An uninitialized reward will always have a reward growth of zero.
pub fn next_reward_growths_inside(
	tick_current_index: i32,
	tick_lower: &Tick,
	tick_lower_index: i32,
	tick_upper: &Tick,
	tick_upper_index: i32,
	reward_infos: &[AMMRewardInfo; NUM_REWARDS]
) -> [u128; NUM_REWARDS] {
	let mut reward_growths_inside = [0; NUM_REWARDS];

	for i in 0..NUM_REWARDS {
		if !reward_infos[i].initialized() {
			continue;
		}

		// By convention, assume all prior growth happened below the tick
		let reward_growths_below = if !tick_lower.initialized {
			reward_infos[i].growth_global_x64
		} else if tick_current_index < tick_lower_index {
			reward_infos[i].growth_global_x64.wrapping_sub(
				tick_lower.reward_growths_outside[i]
			)
		} else {
			tick_lower.reward_growths_outside[i]
		};

		// By convention, assume all prior growth happened below the tick, not above
		let reward_growths_above = if !tick_upper.initialized {
			0
		} else if tick_current_index < tick_upper_index {
			tick_upper.reward_growths_outside[i]
		} else {
			reward_infos[i].growth_global_x64.wrapping_sub(
				tick_upper.reward_growths_outside[i]
			)
		};

		reward_growths_inside[i] = reward_infos[i].growth_global_x64
			.wrapping_sub(reward_growths_below)
			.wrapping_sub(reward_growths_above);
	}

	reward_growths_inside
}
