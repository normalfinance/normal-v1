use anchor_lang::prelude::*;
use anchor_spl::memo::Memo;
use anchor_spl::token_interface::{ Mint, TokenAccount, TokenInterface };

use crate::errors::ErrorCode;

use crate::state::amm::AMM;
use crate::math::{ self, convert_to_liquidity_delta };
use crate::controller;
use crate::util::{
	calculate_transfer_fee_included_amount,
	mint_synthetic_to_vault_v2,
	parse_remaining_accounts,
	AccountsType,
	RemainingAccountsInfo,
};
use crate::util::{
	to_timestamp_u64,
	v2::transfer_from_owner_to_vault_v2,
	verify_position_authority_interface,
};

#[derive(Accounts)]
pub struct ModifyLiquidityV2<'info> {
	#[account(mut)]
	pub amm: Account<'info, AMM>,

	#[account(address = *token_mint_a.to_account_info().owner)]
	pub token_program_a: Interface<'info, TokenInterface>,
	#[account(address = *token_mint_b.to_account_info().owner)]
	pub token_program_b: Interface<'info, TokenInterface>,

	pub memo_program: Program<'info, Memo>,

	pub position_authority: Signer<'info>,

	#[account(mut, has_one = amm)]
	pub position: Account<'info, LiquidityPosition>,
	#[account(
		constraint = position_token_account.mint == position.position_mint,
		constraint = position_token_account.amount == 1
	)]
	pub position_token_account: Box<InterfaceAccount<'info, TokenAccount>>,

	#[account(address = amm.token_mint_a)]
	pub token_mint_a: InterfaceAccount<'info, Mint>,
	#[account(address = amm.token_mint_b)]
	pub token_mint_b: InterfaceAccount<'info, Mint>,

	#[account(mut, constraint = token_owner_account_a.mint == amm.token_mint_a)]
	pub token_owner_account_a: Box<InterfaceAccount<'info, TokenAccount>>,
	#[account(mut, constraint = token_owner_account_b.mint == amm.token_mint_b)]
	pub token_owner_account_b: Box<InterfaceAccount<'info, TokenAccount>>,

	#[account(mut, constraint = token_vault_a.key() == amm.token_vault_a)]
	pub token_vault_a: Box<InterfaceAccount<'info, TokenAccount>>,
	#[account(mut, constraint = token_vault_b.key() == amm.token_vault_b)]
	pub token_vault_b: Box<InterfaceAccount<'info, TokenAccount>>,

	#[account(mut, has_one = amm)]
	pub tick_array_lower: AccountLoader<'info, TickArray>,
	#[account(mut, has_one = amm)]
	pub tick_array_upper: AccountLoader<'info, TickArray>,
	// remaining accounts
	// - accounts for transfer hook program of token_mint_a
	// - accounts for transfer hook program of token_mint_b
}

pub fn handle_increase_liquidity_v2<'info>(
	ctx: Context<'_, '_, '_, 'info, ModifyLiquidityV2<'info>>,
	liquidity_amount: u128,
	token_max_b: u64,
	remaining_accounts_info: Option<RemainingAccountsInfo>
) -> Result<()> {
	verify_position_authority_interface(
		&ctx.accounts.position_token_account,
		&ctx.accounts.position_authority
	)?;

	let clock = Clock::get()?;

	if liquidity_amount == 0 {
		return Err(ErrorCode::LiquidityZero.into());
	}

	// Process remaining accounts
	let remaining_accounts = parse_remaining_accounts(
		ctx.remaining_accounts,
		&remaining_accounts_info,
		&[AccountsType::TransferHookA, AccountsType::TransferHookB]
	)?;

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

	let transfer_fee_included_delta_b = calculate_transfer_fee_included_amount(
		&ctx.accounts.token_mint_b,
		delta_b
	)?;

	// token_max_b should be applied to the transfer fee included amount
	if transfer_fee_included_delta_b.amount > token_max_b {
		return Err(ErrorCode::TokenMaxExceeded.into());
	}

	// Mint a delta_a amount of the AMM's synthetic token to match the user's liquidity
	mint_synthetic_to_vault_v2(
		&ctx.accounts.position_authority,
		&ctx.accounts.token_vault_a,
		&ctx.accounts.token_program,
		delta_a
	)?;
	// transfer_from_owner_to_vault_v2(
	// 	&ctx.accounts.position_authority,
	// 	&ctx.accounts.token_mint_a,
	// 	&ctx.accounts.token_owner_account_a,
	// 	&ctx.accounts.token_vault_a,
	// 	&ctx.accounts.token_program_a,
	// 	&ctx.accounts.memo_program,
	// 	&remaining_accounts.transfer_hook_a,
	// 	transfer_fee_included_delta_a.amount
	// )?;

	transfer_from_owner_to_vault_v2(
		&ctx.accounts.position_authority,
		&ctx.accounts.token_mint_b,
		&ctx.accounts.token_owner_account_b,
		&ctx.accounts.token_vault_b,
		&ctx.accounts.token_program_b,
		&ctx.accounts.memo_program,
		&remaining_accounts.transfer_hook_b,
		transfer_fee_included_delta_b.amount
	)?;

	Ok(())
}
