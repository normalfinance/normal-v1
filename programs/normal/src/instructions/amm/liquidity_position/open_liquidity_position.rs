use anchor_lang::prelude::*;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token::{ self, Mint, Token, TokenAccount };
use synth_market::SynthMarket;

use crate::state;
use crate::{ state::*, util::mint_position_token_and_remove_authority };

#[derive(Accounts)]
pub struct OpenLiquidityPosition<'info> {
	#[account(mut)]
	pub funder: Signer<'info>,

	/// CHECK: safe, the account that will be the owner of the position can be arbitrary
	pub owner: UncheckedAccount<'info>,

	#[account(
		init,
		payer = funder,
		space = Position::LEN,
		seeds = [b"position".as_ref(), position_mint.key().as_ref()],
		bump
	)]
	pub position: Box<Account<'info, Position>>,

	#[account(init, payer = funder, mint::authority = market, mint::decimals = 0)]
	pub position_mint: Account<'info, Mint>,

	#[account(
		init,
		payer = funder,
		associated_token::mint = position_mint,
		associated_token::authority = owner
	)]
	pub position_token_account: Box<Account<'info, TokenAccount>>,

	pub market: Box<Account<'info, Market>>,

	#[account(address = token::ID)]
	pub token_program: Program<'info, Token>,
	pub system_program: Program<'info, System>,
	pub rent: Sysvar<'info, Rent>,
	pub associated_token_program: Program<'info, AssociatedToken>,
}

/*
  Opens a new Market AMM Position.
*/
pub fn handle_open_liquidity_position(
	ctx: Context<OpenLiquidityPosition>,
	tick_lower_index: i32,
	tick_upper_index: i32
) -> Result<()> {
	let market = &ctx.accounts.market;
	let position_mint = &ctx.accounts.position_mint;
	let position = &mut ctx.accounts.position;

	position.open_position(
		market,
		position_mint.key(),
		tick_lower_index,
		tick_upper_index
	)?;

	mint_position_token_and_remove_authority(
		market,
		position_mint,
		&ctx.accounts.position_token_account,
		&ctx.accounts.token_program
	)
}
