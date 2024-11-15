use anchor_lang::prelude::*;
use anchor_spl::memo::Memo;
use anchor_spl::token_interface::{ Mint, TokenAccount, TokenInterface };

use crate::errors::ErrorCode;
use crate::manager::liquidity_manager::{
	calculate_liquidity_token_deltas,
	calculate_modify_liquidity,
	sync_modify_liquidity_values,
};
use crate::math::{ self, convert_to_liquidity_delta };
use crate::{ controller, state::* };
use crate::util::{
	calculate_transfer_fee_included_amount,
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

	#[account(address = *token_mint_synthetic.to_account_info().owner)]
	pub token_program_synthetic: Interface<'info, TokenInterface>,
	#[account(address = *token_mint_quote.to_account_info().owner)]
	pub token_program_quote: Interface<'info, TokenInterface>,

	pub memo_program: Program<'info, Memo>,

	pub position_authority: Signer<'info>,

	#[account(mut, has_one = amm)]
	pub position: Account<'info, Position>,
	#[account(
		constraint = position_token_account.mint == position.position_mint,
		constraint = position_token_account.amount == 1
	)]
	pub position_token_account: Box<InterfaceAccount<'info, TokenAccount>>,

	#[account(address = amm.token_mint_synthetic)]
	pub token_mint_synthetic: InterfaceAccount<'info, Mint>,
	#[account(address = amm.token_mint_quote)]
	pub token_mint_quote: InterfaceAccount<'info, Mint>,

	#[account(mut, constraint = token_owner_account_quote.mint == amm.token_mint_quote)]
	pub token_owner_account_quote: Box<InterfaceAccount<'info, TokenAccount>>,

	#[account(mut, constraint = token_vault_synthetic.key() == amm.token_vault_synthetic)]
	pub token_vault_synthetic: Box<InterfaceAccount<'info, TokenAccount>>,
	#[account(mut, constraint = token_vault_quote.key() == amm.token_vault_quote)]
	pub token_vault_quote: Box<InterfaceAccount<'info, TokenAccount>>,

	#[account(mut, has_one = amm)]
	pub tick_array_lower: AccountLoader<'info, TickArray>,
	#[account(mut, has_one = amm)]
	pub tick_array_upper: AccountLoader<'info, TickArray>,
	// remaining accounts
	// - accounts for transfer hook program of token_mint_synthetic
	// - accounts for transfer hook program of token_mint_quote
}

pub fn handler<'info>(
	ctx: Context<'_, '_, '_, 'info, ModifyLiquidityV2<'info>>,
	liquidity_amount: u128,
	token_max_quote: u64,
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

	let liquidity_delta = math::amm::convert_to_liquidity_delta(
		liquidity_amount,
		true
	)?;
	let timestamp = to_timestamp_u64(clock.unix_timestamp)?;

	let update = controller::liquidity::calculate_modify_liquidity(
		&ctx.accounts.amm,
		&ctx.accounts.position,
		&ctx.accounts.tick_array_lower,
		&ctx.accounts.tick_array_upper,
		liquidity_delta,
		timestamp
	)?;

	controller::liquidity::sync_modify_liquidity_values(
		&mut ctx.accounts.amm,
		&mut ctx.accounts.position,
		&ctx.accounts.tick_array_lower,
		&ctx.accounts.tick_array_upper,
		update,
		timestamp
	)?;

	let (delta_synthetic, delta_quote) =
		controller::liquidity::calculate_liquidity_token_deltas(
			&ctx.accounts.amm,
			ctx.accounts.amm.tick_current_index,
			// ctx.accounts.amm.sqrt_price,
			ctx.accounts.amm.historical_oracle_data.last_oracle_price, // Use the oracle price instead of the sqrt_price to maintain peg
			&ctx.accounts.position,
			liquidity_delta
		)?;

	let transfer_fee_included_delta_quote =
		calculate_transfer_fee_included_amount(
			&ctx.accounts.token_mint_quote,
			delta_quote
		)?;

	// token_max_quote should be applied to the transfer fee included amount
	if transfer_fee_included_delta_quote.amount > token_max_quote {
		return Err(ErrorCode::TokenMaxExceeded.into());
	}

	// Mint a delta_synthetic amount of the AMM's synthetic token to match the user's liquidity
	mint_synthetic_to_vault_v2(
		position_authority,
		token_owner_account,
		token_vault,
		token_program,
		amount
	)?;
	// transfer_from_owner_to_vault_v2(
	//     &ctx.accounts.position_authority,
	//     &ctx.accounts.token_mint_synthetic,
	//     &ctx.accounts.token_owner_account_synthetic,
	//     &ctx.accounts.token_vault_synthetic,
	//     &ctx.accounts.token_program_synthetic,
	//     &ctx.accounts.memo_program,
	//     &remaining_accounts.transfer_hook_a,
	//     transfer_fee_included_delta_synthetic.amount,
	// )?;

	transfer_from_owner_to_vault_v2(
		&ctx.accounts.position_authority,
		&ctx.accounts.token_mint_quote,
		&ctx.accounts.token_owner_account_quote,
		&ctx.accounts.token_vault_quote,
		&ctx.accounts.token_program_quote,
		&ctx.accounts.memo_program,
		&remaining_accounts.transfer_hook_b,
		transfer_fee_included_delta_quote.amount
	)?;

	Ok(())
}
