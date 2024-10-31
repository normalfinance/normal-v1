use anchor_lang::prelude::*;
use anchor_spl::token::{ self, Mint, Token, TokenAccount };

use crate::state::amm::AMM;

#[derive(Accounts)]
// now we don't use bumps, but we must list args in the same order to use tick_spacing arg.
#[instruction(bumps: AMMBumps, tick_spacing: u16)]
pub struct InitializeAMM<'info> {
	pub token_mint_a: Account<'info, Mint>,
	pub token_mint_b: Account<'info, Mint>,

	#[account(mut)]
	pub funder: Signer<'info>,

	#[account(
		init,
		seeds = [
			b"amm".as_ref(),
			token_mint_a.key().as_ref(),
			token_mint_b.key().as_ref(),
			tick_spacing.to_le_bytes().as_ref(),
		],
		bump,
		payer = funder,
		space = AMM::LEN
	)]
	pub amm: Box<Account<'info, AMM>>,

	#[account(
		init,
		payer = funder,
		token::mint = token_mint_a,
		token::authority = amm
	)]
	pub token_vault_a: Box<Account<'info, TokenAccount>>,

	#[account(
		init,
		payer = funder,
		token::mint = token_mint_b,
		token::authority = amm
	)]
	pub token_vault_b: Box<Account<'info, TokenAccount>>,

	#[account(address = token::ID)]
	pub token_program: Program<'info, Token>,
	pub system_program: Program<'info, System>,
	pub rent: Sysvar<'info, Rent>,
}

pub fn handle_initialize_amm(
	ctx: Context<InitializeAMM>,
	_bumps: AMMBumps,
	tick_spacing: u16,
	initial_sqrt_price: u128
) -> Result<()> {
	let token_mint_a = ctx.accounts.token_mint_a.key();
	let token_mint_b = ctx.accounts.token_mint_b.key();

	let amm = &mut ctx.accounts.amm;

	// ignore the bump passed and use one Anchor derived
	let bump = ctx.bumps.amm;

	amm.initialize(
		bump,
		tick_spacing,
		initial_sqrt_price,
		default_fee_rate,
		token_mint_a,
		ctx.accounts.token_vault_a.key(),
		token_mint_b,
		ctx.accounts.token_vault_b.key()
	)
}
