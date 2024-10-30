use anchor_lang::prelude::*;
use anchor_spl::token::TokenAccount;

use crate::controller;
use crate::errors::ErrorCode;
use crate::math::checked_mul_shift_right;
use crate::state::amm::AMM;
use crate::util::to_timestamp_u64;

const DAY_IN_SECONDS: u128 = 60 * 60 * 24;

#[derive(Accounts)]
#[instruction(reward_index: u8)]
pub struct SetAMMRewardEmissions<'info> {
	#[account(mut)]
	pub amm: Account<'info, AMM>,

	#[account(address = amm.reward_infos[reward_index as usize].authority)]
	pub reward_authority: Signer<'info>,

	#[account(address = amm.reward_infos[reward_index as usize].vault)]
	pub reward_vault: Account<'info, TokenAccount>,
}

pub fn handle_set_amm_reward_emissions(
	ctx: Context<SetAMMRewardEmissions>,
	reward_index: u8,
	emissions_per_second_x64: u128
) -> Result<()> {
	let amm = &ctx.accounts.amm;
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
	let next_reward_infos = controller::lp::next_amm_reward_infos(
		amm,
		timestamp
	)?;

	ctx.accounts.amm.update_emissions(
		reward_index as usize,
		next_reward_infos,
		timestamp,
		emissions_per_second_x64
	)
}
