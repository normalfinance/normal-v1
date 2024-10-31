use anchor_lang::prelude::*;

use crate::constants::transfer_memo;
use crate::{ controller, math };
use crate::errors::ErrorCode;
use crate::manager::liquidity_manager::{
	calculate_liquidity_token_deltas,
	calculate_modify_liquidity,
	sync_modify_liquidity_values,
};
use crate::math::convert_to_liquidity_delta;
use crate::util::{
	burn_synthetic_from_vault_v2,
	calculate_transfer_fee_excluded_amount,
	parse_remaining_accounts,
	AccountsType,
	RemainingAccountsInfo,
};
use crate::util::{
	to_timestamp_u64,
	v2::transfer_from_vault_to_owner_v2,
	verify_position_authority_interface,
};

use super::increase_liquidity::ModifyLiquidityV2;

/*
  Removes liquidity from an existing AMM Position.
*/
pub fn handle_decrease_liquidity_v2<'info>(
	ctx: Context<'_, '_, '_, 'info, ModifyLiquidityV2<'info>>,
	liquidity_amount: u128,
	token_min_b: u64,
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
		false
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

	let transfer_fee_excluded_delta_b = calculate_transfer_fee_excluded_amount(
		&ctx.accounts.token_mint_b,
		delta_b
	)?;

	// token_min_b should be applied to the transfer fee excluded amount
	if transfer_fee_excluded_delta_b.amount < token_min_b {
		return Err(ErrorCode::TokenMinSubceeded.into());
	}

	// Burn a delta_a amount of synthetic tokens from the AMM
	burn_synthetic_from_vault_v2(
		authority,
		token_owner_account,
		token_vault,
		token_program,
		amount
	)?;
	// transfer_from_vault_to_owner_v2(
	// 	&ctx.accounts.amm,
	// 	&ctx.accounts.token_mint_a,
	// 	&ctx.accounts.token_vault_a,
	// 	&ctx.accounts.token_owner_account_a,
	// 	&ctx.accounts.token_program_a,
	// 	&ctx.accounts.memo_program,
	// 	&remaining_accounts.transfer_hook_a,
	// 	delta_a,
	// 	transfer_memo::TRANSFER_MEMO_DECREASE_LIQUIDITY.as_bytes()
	// )?;

	transfer_from_vault_to_owner_v2(
		&ctx.accounts.amm,
		&ctx.accounts.token_mint_b,
		&ctx.accounts.token_vault_b,
		&ctx.accounts.token_owner_account_b,
		&ctx.accounts.token_program_b,
		&ctx.accounts.memo_program,
		&remaining_accounts.transfer_hook_b,
		delta_b,
		transfer_memo::TRANSFER_MEMO_DECREASE_LIQUIDITY.as_bytes()
	)?;

	Ok(())
}