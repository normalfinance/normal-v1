use anchor_lang::prelude::*;
use anchor_spl::token::TokenAccount;

use crate::controller;
use crate::errors::ErrorCode;
use crate::manager::amm_manager::next_amm_reward_infos;
use crate::math::checked_mul_shift_right;
use crate::state::amm::NUM_REWARDS;
use crate::state::market::Market;
use crate::util::to_timestamp_u64;

const DAY_IN_SECONDS: u128 = 60 * 60 * 24;

#[derive(Accounts)]
#[instruction(reward_index: u8)]
pub struct SetRewardEmissions<'info> {
	#[account(mut)]
	pub market: AccountLoader<'info, Market>,

	#[account(address = market.amm.reward_infos[reward_index as usize].authority)]
	pub reward_authority: Signer<'info>,

	#[account(address = market.amm.reward_infos[reward_index as usize].vault)]
	pub reward_vault: Account<'info, TokenAccount>,
}

pub fn handle_set_reward_emissions(
	ctx: Context<SetRewardEmissions>,
	reward_index: u8,
	emissions_per_second_x64: u128
) -> Result<()> {
	let market = &mut ctx.accounts.market.load_init()?;
	let reward_vault = &ctx.accounts.reward_vault;

	let emissions_per_day = checked_mul_shift_right(
		DAY_IN_SECONDS,
		emissions_per_second_x64
	)?;
	if reward_vault.amount < emissions_per_day {
		return Err(ErrorCode::RewardVaultAmountInsufficient.into());
	}

	let clock = Clock::get()?;
	let timestamp = to_timestamp_u64(clock.unix_timestamp)?;
	let next_reward_infos = controller::amm::next_amm_reward_infos(
		market.amm,
		timestamp
	)?;

	let index: usize = reward_index as usize;

	if index >= NUM_REWARDS {
		return Err(ErrorCode::InvalidRewardIndex.into());
	}
	market.amm.update_rewards(next_reward_infos, timestamp);
	market.amm.reward_infos[index].emissions_per_second_x64 =
		emissions_per_second_x64;

	Ok(())
}
