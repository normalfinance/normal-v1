use amm::AMM;
use anchor_lang::prelude::*;
use anchor_spl::token::{ self, Token, TokenAccount };
use anchor_spl::token_interface::TokenAccount as TokenAccountInterface;
use liquidity_position::LiquidityPosition;

use crate::{
	state::*,
	util::{ transfer_from_vault_to_owner, verify_position_authority_interface },
};

#[derive(Accounts)]
pub struct CollectLiquidityPositionFees<'info> {
	pub amm: Box<Account<'info, AMM>>,

	pub position_authority: Signer<'info>,

	#[account(mut, has_one = amm)]
	pub position: Box<Account<'info, LiquidityPosition>>,
	#[account(
		constraint = position_token_account.mint == position.position_mint,
		constraint = position_token_account.amount == 1
	)]
	pub position_token_account: Box<
		InterfaceAccount<'info, TokenAccountInterface>
	>,

	#[account(mut, constraint = token_owner_account_a.mint == amm.token_mint_a)]
	pub token_owner_account_a: Box<Account<'info, TokenAccount>>,
	#[account(mut, address = amm.token_vault_a)]
	pub token_vault_a: Box<Account<'info, TokenAccount>>,

	#[account(mut, constraint = token_owner_account_b.mint == amm.token_mint_b)]
	pub token_owner_account_b: Box<Account<'info, TokenAccount>>,
	#[account(mut, address = amm.token_vault_b)]
	pub token_vault_b: Box<Account<'info, TokenAccount>>,

	#[account(address = token::ID)]
	pub token_program: Program<'info, Token>,
}

pub fn handle_collect_liquidity_position_fees(
	ctx: Context<CollectLiquidityPositionFees>
) -> Result<()> {
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
		&ctx.accounts.amm,
		&ctx.accounts.token_vault_a,
		&ctx.accounts.token_owner_account_a,
		&ctx.accounts.token_program,
		fee_owed_a
	)?;

	transfer_from_vault_to_owner(
		&ctx.accounts.amm,
		&ctx.accounts.token_vault_b,
		&ctx.accounts.token_owner_account_b,
		&ctx.accounts.token_program,
		fee_owed_b
	)?;

	Ok(())
}
