use anchor_lang::prelude::*;
use anchor_spl::token::{ self, Token, TokenAccount };

use crate::{
	controller,
	errors::ErrorCode,
	manager::swap_manager::*,
	state::{ market::Market, AMM },
	util::{
		to_timestamp_u64,
		update_and_swap_amm,
		SparseSwapTickSequenceBuilder,
	},
};

#[derive(Accounts)]
pub struct Swap<'info> {
	#[account(address = token::ID)]
	pub token_program: Program<'info, Token>,

	pub token_authority: Signer<'info>,

	#[account(mut)]
	pub amm: Box<Account<'info, AMM>>,

	#[account(mut, constraint = token_owner_account_synthetic.mint == amm.token_mint_synthetic)]
	pub token_owner_account_synthetic: Box<Account<'info, TokenAccount>>,
	#[account(mut, address = amm.token_vault_synthetic)]
	pub token_vault_synthetic: Box<Account<'info, TokenAccount>>,

	#[account(mut, constraint = token_owner_account_quote.mint == amm.token_mint_quote)]
	pub token_owner_account_quote: Box<Account<'info, TokenAccount>>,
	#[account(mut, address = amm.token_vault_quote)]
	pub token_vault_quote: Box<Account<'info, TokenAccount>>,

	#[account(mut)]
	/// CHECK: checked in the handler
	pub tick_array_0: UncheckedAccount<'info>,

	#[account(mut)]
	/// CHECK: checked in the handler
	pub tick_array_1: UncheckedAccount<'info>,

	#[account(mut)]
	/// CHECK: checked in the handler
	pub tick_array_2: UncheckedAccount<'info>,
}

pub fn handle_swap(
	ctx: Context<Swap>,
	amount: u64,
	other_amount_threshold: u64,
	sqrt_price_limit: u128,
	amount_specified_is_input: bool,
	synthetic_to_quote: bool // Zero for one
) -> Result<()> {
	let amm = &mut ctx.accounts.amm;
	let clock = Clock::get()?;
	// Update the global reward growth which increases as a function of time.
	let timestamp = to_timestamp_u64(clock.unix_timestamp)?;

	let builder = SparseSwapTickSequenceBuilder::try_from(
		amm,
		synthetic_to_quote,
		vec![
			ctx.accounts.tick_array_0.to_account_info(),
			ctx.accounts.tick_array_1.to_account_info(),
			ctx.accounts.tick_array_2.to_account_info()
		],
		None
	)?;
	let mut swap_tick_sequence = builder.build()?;

	let swap_update = controller::swap::swap(
		amm,
		&mut swap_tick_sequence,
		amount,
		sqrt_price_limit,
		amount_specified_is_input,
		synthetic_to_quote,
		timestamp
	)?;

	if amount_specified_is_input {
		if
			(synthetic_to_quote &&
				other_amount_threshold > swap_update.amount_quote) ||
			(!synthetic_to_quote &&
				other_amount_threshold > swap_update.amount_synthetic)
		{
			return Err(ErrorCode::AmountOutBelowMinimum.into());
		}
	} else if
		(synthetic_to_quote &&
			other_amount_threshold < swap_update.amount_synthetic) ||
		(!synthetic_to_quote && other_amount_threshold < swap_update.amount_quote)
	{
		return Err(ErrorCode::AmountInAboveMaximum.into());
	}

	let inside_range = amm.is_price_inside_range(
		swap_update.next_sqrt_price
	);

	update_and_swap_amm(
		amm,
		&ctx.accounts.token_authority,
		&ctx.accounts.token_owner_account_synthetic,
		&ctx.accounts.token_owner_account_quote,
		&ctx.accounts.token_vault_synthetic,
		&ctx.accounts.token_vault_quote,
		&ctx.accounts.token_program,
		swap_update,
		synthetic_to_quote,
		timestamp,
		inside_range
	)
}
