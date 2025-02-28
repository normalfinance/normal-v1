use anchor_lang::prelude::*;
use anchor_spl::token::{ self, Token, TokenAccount };

use crate::{ state::*, util::transfer_from_vault_to_owner };
use synth_market::SynthMarket;

#[derive(Accounts)]
pub struct CollectProtocolFees<'info> {
	#[account(mut)]
	pub market: AccountLoader<'info, Market>,

	#[account(mut, address = amm.token_vault_synthetic)]
	pub token_vault_synthetic: Account<'info, TokenAccount>,

	#[account(mut, address = amm.token_vault_quote)]
	pub token_vault_quote: Account<'info, TokenAccount>,

	#[account(mut, constraint = token_destination_a.mint == amm.token_mint_synthetic)]
	pub token_destination_a: Account<'info, TokenAccount>,

	#[account(mut, constraint = token_destination_b.mint == amm.token_mint_quote)]
	pub token_destination_b: Account<'info, TokenAccount>,

	#[account(address = token::ID)]
	pub token_program: Program<'info, Token>,
}

pub fn handle_collect_protocol_fees(
	ctx: Context<CollectProtocolFees>
) -> Result<()> {
	let market = &ctx.accounts.market;

	transfer_from_vault_to_owner(
		market,
		&ctx.accounts.token_vault_synthetic,
		&ctx.accounts.token_destination_a,
		&ctx.accounts.token_program,
		amm.protocol_fee_owed_synthetic
	)?;

	transfer_from_vault_to_owner(
		market,
		&ctx.accounts.token_vault_quote,
		&ctx.accounts.token_destination_b,
		&ctx.accounts.token_program,
		amm.protocol_fee_owed_quote
	)?;

	ctx.accounts.amm.reset_protocol_fees_owed();

	Ok(())
}
