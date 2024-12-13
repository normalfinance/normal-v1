use anchor_lang::prelude::*;
use anchor_spl::token::{ self, Token, TokenAccount };
use anchor_spl::token_interface::TokenAccount as TokenAccountInterface;
use lp::LP;
use market::Market;

use crate::{
	state::*,
	util::{ transfer_from_vault_to_owner, verify_position_authority_interface },
};

#[derive(Accounts)]
pub struct CollectFees<'info> {
	#[account(mut)]
	pub market: AccountLoader<'info, Market>,

	pub position_authority: Signer<'info>,

	#[account(mut, has_one = market)]
	pub position: Box<Account<'info, LP>>,
	#[account(
		constraint = position_token_account.mint == position.position_mint,
		constraint = position_token_account.amount == 1
	)]
	pub position_token_account: Box<
		InterfaceAccount<'info, TokenAccountInterface>
	>,

	#[account(mut, constraint = token_owner_account_synthetic.mint == market.amm.token_mint_synthetic)]
	pub token_owner_account_synthetic: Box<Account<'info, TokenAccount>>,
	#[account(mut, address = market.amm.token_vault_synthetic)]
	pub token_vault_synthetic: Box<Account<'info, TokenAccount>>,

	#[account(mut, constraint = token_owner_account_quote.mint == market.amm.token_mint_quote)]
	pub token_owner_account_quote: Box<Account<'info, TokenAccount>>,
	#[account(mut, address = market.amm.token_vault_quote)]
	pub token_vault_quote: Box<Account<'info, TokenAccount>>,

	#[account(address = token::ID)]
	pub token_program: Program<'info, Token>,
}

pub fn handle_collect_fees(ctx: Context<CollectFees>) -> Result<()> {
	verify_position_authority_interface(
		&ctx.accounts.position_token_account,
		&ctx.accounts.position_authority
	)?;

	let position = &mut ctx.accounts.position;

	// Store the fees owed to use as transfer amounts.
	let fee_owed_a = position.fee_owed_a;
	let fee_owed_b = position.fee_owed_b;

	position.reset_fees_owed();

	transfer_from_vault_to_owner(
		&ctx.accounts.market,
		&ctx.accounts.token_vault_synthetic,
		&ctx.accounts.token_owner_account_synthetic,
		&ctx.accounts.token_program,
		fee_owed_a
	)?;

	transfer_from_vault_to_owner(
		&ctx.accounts.market,
		&ctx.accounts.token_vault_quote,
		&ctx.accounts.token_owner_account_quote,
		&ctx.accounts.token_program,
		fee_owed_b
	)?;

	Ok(())
}
