use amm::AMM;
use anchor_lang::prelude::*;
use anchor_spl::token::{ self, Token, TokenAccount };
use anchor_spl::token_interface::TokenAccount as TokenAccountInterface;
use crate::state::liquidity_position::LiquidityPosition;
use market::Market;

use crate::errors::ErrorCode;
use crate::manager::liquidity_manager::{
	calculate_liquidity_token_deltas,
	calculate_modify_liquidity,
	sync_modify_liquidity_values,
};
use crate::math::{ self, convert_to_liquidity_delta };
use crate::{ controller, state::* };
use crate::util::{
	to_timestamp_u64,
	mint_synthetic_to_vault,
	transfer_from_owner_to_vault,
	verify_position_authority_interface,
};

#[derive(Accounts)]
pub struct ModifyLiquidity<'info> {
	#[account(mut)]
	pub amm: Account<'info, AMM>,

	#[account(address = token::ID)]
	pub token_program: Program<'info, Token>,

	pub position_authority: Signer<'info>,

	#[account(mut, has_one = amm)]
	pub position: Account<'info, LiquidityPosition>,
	#[account(
		constraint = position_token_account.mint == position.position_mint,
		constraint = position_token_account.amount == 1
	)]
	pub position_token_account: Box<
		InterfaceAccount<'info, TokenAccountInterface>
	>,

	#[account(mut, constraint = token_owner_account_a.mint == amm.token_mint_a)]
	pub token_owner_account_a: Box<Account<'info, TokenAccount>>,
	#[account(mut, constraint = token_owner_account_b.mint == amm.token_mint_b)]
	pub token_owner_account_b: Box<Account<'info, TokenAccount>>,

	#[account(mut, constraint = token_vault_a.key() == amm.token_vault_a)]
	pub token_vault_a: Box<Account<'info, TokenAccount>>,
	#[account(mut, constraint = token_vault_b.key() == amm.token_vault_b)]
	pub token_vault_b: Box<Account<'info, TokenAccount>>,

	#[account(mut, has_one = amm)]
	pub tick_array_lower: AccountLoader<'info, TickArray>,
	#[account(mut, has_one = amm)]
	pub tick_array_upper: AccountLoader<'info, TickArray>,
}

pub fn handle_increase_liquidity(
	ctx: Context<ModifyLiquidity>,
	liquidity_amount: u128,
	token_max_b: u64
) -> Result<()> {
	verify_position_authority_interface(
		&ctx.accounts.position_token_account,
		&ctx.accounts.position_authority
	)?;

	let clock = Clock::get()?;

	if liquidity_amount == 0 {
		return Err(ErrorCode::LiquidityZero.into());
	}

	let liquidity_delta = math::lp::convert_to_liquidity_delta(
		liquidity_amount,
		true
	)?;
	let timestamp = to_timestamp_u64(clock.unix_timestamp)?;

	let update = controller::lp::calculate_modify_liquidity(
		&ctx.accounts.amm,
		&ctx.accounts.position,
		&ctx.accounts.tick_array_lower,
		&ctx.accounts.tick_array_upper,
		liquidity_delta,
		timestamp
	)?;

	controller::lp::sync_modify_liquidity_values(
		&mut ctx.accounts.amm,
		&mut ctx.accounts.position,
		&ctx.accounts.tick_array_lower,
		&ctx.accounts.tick_array_upper,
		update,
		timestamp
	)?;

	let (delta_a, delta_b) = controller::lp::calculate_liquidity_token_deltas(
		ctx.accounts.amm.tick_current_index,
		ctx.accounts.amm.sqrt_price,
		&ctx.accounts.position,
		liquidity_delta
	)?;

	if delta_b > token_max_b {
		return Err(ErrorCode::TokenMaxExceeded.into());
	}

	// Mint a delta_a amount of the AMM's synthetic token to match the user's liquidity
	mint_synthetic_to_vault(
		&ctx.accounts.position_authority,
		&ctx.accounts.token_vault_a,
		&ctx.accounts.token_program,
		delta_a
	)?;
	// transfer_from_owner_to_vault(
	// 	&ctx.accounts.position_authority,
	// 	&ctx.accounts.token_owner_account_a,
	// 	&ctx.accounts.token_vault_a,
	// 	&ctx.accounts.token_program,
	// 	delta_a
	// )?;

	transfer_from_owner_to_vault(
		&ctx.accounts.position_authority,
		&ctx.accounts.token_owner_account_b,
		&ctx.accounts.token_vault_b,
		&ctx.accounts.token_program,
		delta_b
	)?;

	Ok(())
}