use amm::AMM;
use anchor_lang::prelude::*;
use crate::state::liquidity_position::LiquidityPosition;

use crate::{ controller, state::*, util::to_timestamp_u64 };

#[derive(Accounts)]
pub struct UpdateAMMFeesAndRewards<'info> {
	#[account(mut)]
	pub amm: Account<'info, AMM>,

	#[account(mut, has_one = amm)]
	pub position: Account<'info, LiquidityLiquidityPosition>,

	#[account(has_one = amm)]
	pub tick_array_lower: AccountLoader<'info, TickArray>,
	#[account(has_one = amm)]
	pub tick_array_upper: AccountLoader<'info, TickArray>,
}

pub fn handle_update_amm_fees_and_rewards(ctx: Context<UpdateAMMFeesAndRewards>) -> Result<()> {
	let amm = &mut ctx.accounts.amm;
	let position = &mut ctx.accounts.position;
	let clock = Clock::get()?;
	let timestamp = to_timestamp_u64(clock.unix_timestamp)?;

	let (position_update, reward_infos) =
		controller::lp::calculate_fee_and_reward_growths(
			amm,
			position,
			&ctx.accounts.tick_array_lower,
			&ctx.accounts.tick_array_upper,
			timestamp
		)?;

	amm.update_rewards(reward_infos, timestamp);
	position.update(&position_update);

	Ok(())
}
