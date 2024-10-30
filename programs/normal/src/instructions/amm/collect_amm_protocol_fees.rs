use crate::{ state::*, util::transfer_from_vault_to_owner };
use amm::AMM;
use anchor_lang::prelude::*;
use anchor_spl::token::{ self, Token, TokenAccount };

#[derive(Accounts)]
pub struct CollectAMMProtocolFees<'info> {
	#[account(mut)]
	pub amm: Box<Account<'info, AMM>>,

	#[account(mut, address = amm.token_vault_a)]
	pub token_vault_a: Account<'info, TokenAccount>,

	#[account(mut, address = amm.token_vault_b)]
	pub token_vault_b: Account<'info, TokenAccount>,

	#[account(mut, constraint = token_destination_a.mint == amm.token_mint_a)]
	pub token_destination_a: Account<'info, TokenAccount>,

	#[account(mut, constraint = token_destination_b.mint == amm.token_mint_b)]
	pub token_destination_b: Account<'info, TokenAccount>,

	#[account(address = token::ID)]
	pub token_program: Program<'info, Token>,
}

pub fn handle_collect_amm_protocol_fees(
	ctx: Context<CollectAMMProtocolFees>
) -> Result<()> {
	let amm = &ctx.accounts.amm;

	transfer_from_vault_to_owner(
		amm,
		&ctx.accounts.token_vault_a,
		&ctx.accounts.token_destination_a,
		&ctx.accounts.token_program,
		amm.protocol_fee_owed_a
	)?;

	transfer_from_vault_to_owner(
		amm,
		&ctx.accounts.token_vault_b,
		&ctx.accounts.token_destination_b,
		&ctx.accounts.token_program,
		amm.protocol_fee_owed_b
	)?;

	ctx.accounts.amm.reset_protocol_fees_owed();
	Ok(())
}
