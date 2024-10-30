use anchor_lang::prelude::*;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token::{ self, Mint, Token, TokenAccount };
use crate::state::liquidity_position_bundle::LiquidityPositionBundle;

use crate::{ state::*, util::mint_position_bundle_token_and_remove_authority };

#[derive(Accounts)]
pub struct InitializeLiquidityPositionBundle<'info> {
	#[account(
		init,
		payer = funder,
		space = LiquidityPositionBundle::LEN,
		seeds = [
			b"liquidity_position_bundle".as_ref(),
			position_bundle_mint.key().as_ref(),
		],
		bump
	)]
	pub position_bundle: Box<Account<'info, LiquidityPositionBundle>>,

	#[account(
		init,
		payer = funder,
		mint::authority = position_bundle, // will be removed in the transaction
		mint::decimals = 0
	)]
	pub position_bundle_mint: Account<'info, Mint>,

	#[account(
		init,
		payer = funder,
		associated_token::mint = position_bundle_mint,
		associated_token::authority = position_bundle_owner
	)]
	pub position_bundle_token_account: Box<Account<'info, TokenAccount>>,

	/// CHECK: safe, the account that will be the owner of the position bundle can be arbitrary
	pub position_bundle_owner: UncheckedAccount<'info>,

	#[account(mut)]
	pub funder: Signer<'info>,

	#[account(address = token::ID)]
	pub token_program: Program<'info, Token>,
	pub system_program: Program<'info, System>,
	pub rent: Sysvar<'info, Rent>,
	pub associated_token_program: Program<'info, AssociatedToken>,
}

pub fn handle_initialize_liquidity_position_bundle(
	ctx: Context<InitializeLiquidityPositionBundle>
) -> Result<()> {
	let position_bundle_mint = &ctx.accounts.position_bundle_mint;
	let position_bundle = &mut ctx.accounts.position_bundle;

	position_bundle.initialize(position_bundle_mint.key())?;

	let bump = ctx.bumps.position_bundle;

	mint_position_bundle_token_and_remove_authority(
		&ctx.accounts.position_bundle,
		position_bundle_mint,
		&ctx.accounts.position_bundle_token_account,
		&ctx.accounts.token_program,
		&[
			b"liquidity_position_bundle".as_ref(),
			position_bundle_mint.key().as_ref(),
			&[bump],
		]
	)
}
