use anchor_lang::prelude::*;
use anchor_spl::token::{ self, Mint, Token, TokenAccount };

use crate::state::amm::AMM;

#[derive(Accounts)]
#[instruction(reward_index: u8)]
pub struct InitializeAMMReward<'info> {
	#[account(address = amm.reward_infos[reward_index as usize].authority)]
	pub reward_authority: Signer<'info>,

	#[account(mut)]
	pub funder: Signer<'info>,

	#[account(mut)]
	pub amm: Box<Account<'info, AMM>>,

	pub reward_mint: Box<Account<'info, Mint>>,

	#[account(
		init,
		payer = funder,
		token::mint = reward_mint,
		token::authority = amm
	)]
	pub reward_vault: Box<Account<'info, TokenAccount>>,

	#[account(address = token::ID)]
	pub token_program: Program<'info, Token>,
	pub system_program: Program<'info, System>,
	pub rent: Sysvar<'info, Rent>,
}

pub fn handle_initialize_amm_reward(
	ctx: Context<InitializeAMMReward>,
	reward_index: u8
) -> Result<()> {
	let amm = &mut ctx.accounts.amm;

	amm.initialize_reward(
		reward_index as usize,
		ctx.accounts.reward_mint.key(),
		ctx.accounts.reward_vault.key()
	)
}
