use anchor_lang::prelude::*;
use anchor_spl::token_interface::{ Mint, TokenAccount, TokenInterface };

use crate::{ errors::ErrorCode, state::AMM };

#[derive(Accounts)]
#[instruction(reward_index: u8)]
pub struct InitializeAMMRewardV2<'info> {
	#[account(address = amm.reward_infos[reward_index as usize].authority)]
	pub reward_authority: Signer<'info>,

	#[account(mut)]
	pub funder: Signer<'info>,

	#[account(mut)]
	pub amm: Box<Account<'info, AMM>>,

	pub reward_mint: Box<InterfaceAccount<'info, Mint>>,

	#[account(
		init,
		payer = funder,
		token::token_program = reward_token_program,
		token::mint = reward_mint,
		token::authority = amm
	)]
	pub reward_vault: Box<InterfaceAccount<'info, TokenAccount>>,

	#[account(address = *reward_mint.to_account_info().owner)]
	pub reward_token_program: Interface<'info, TokenInterface>,
	pub system_program: Program<'info, System>,
	pub rent: Sysvar<'info, Rent>,
}

pub fn handle_initialize_amm_reward_v2(
	ctx: Context<InitializeAMMRewardV2>,
	reward_index: u8
) -> Result<()> {
	let amm = &mut ctx.accounts.amm;

	amm.initialize_reward(
		reward_index as usize,
		ctx.accounts.reward_mint.key(),
		ctx.accounts.reward_vault.key()
	)
}
