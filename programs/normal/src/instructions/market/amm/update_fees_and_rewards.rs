use anchor_lang::prelude::*;
use lp::LP;
use market::Market;
use tick::TickArray;

use crate::{
	controller::{ self },
	manager::liquidity_manager::calculate_fee_and_reward_growths,
	state::*,
	util::to_timestamp_u64,
};

#[derive(Accounts)]
pub struct UpdateFeesAndRewards<'info> {
	#[account(mut)]
	pub market: Account<'info, Market>,

	#[account(mut, has_one = market)]
	pub position: Account<'info, LP>,

	#[account(has_one = market)]
	pub tick_array_lower: AccountLoader<'info, TickArray>,
	#[account(has_one = market)]
	pub tick_array_upper: AccountLoader<'info, TickArray>,
}

pub fn handle_update_fees_and_rewards(
	ctx: Context<UpdateFeesAndRewards>
) -> Result<()> {
	let market = &mut ctx.accounts.market;
	let position = &mut ctx.accounts.position;
	let clock = Clock::get()?;
	let timestamp = to_timestamp_u64(clock.unix_timestamp)?;

	let (position_update, reward_infos) =
		controller::liquidity::calculate_fee_and_reward_growths(
			market.amm,
			position,
			&ctx.accounts.tick_array_lower,
			&ctx.accounts.tick_array_upper,
			timestamp
		)?;

	market.amm.update_rewards(reward_infos, timestamp);
	position.update(&position_update);

	Ok(())
}
