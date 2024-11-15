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
pub fn handler<'info>(
	ctx: Context<'_, '_, '_, 'info, ModifyLiquidityV2<'info>>,
	liquidity_amount: u128,
	token_min_quote: u64,
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
		false
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
			ctx.accounts.amm.tick_current_index,
			ctx.accounts.amm.sqrt_price,
			&ctx.accounts.position,
			liquidity_delta
		)?;

	let transfer_fee_excluded_delta_quote = calculate_transfer_fee_excluded_amount(
		&ctx.accounts.token_mint_quote,
		delta_quote
	)?;

	// token_min_quote should be applied to the transfer fee excluded amount
	if transfer_fee_excluded_delta_quote.amount < token_min_quote {
		return Err(ErrorCode::TokenMinSubceeded.into());
	}

	transfer_from_vault_to_owner_v2(
		&ctx.accounts.amm,
		&ctx.accounts.token_mint_synthetic,
		&ctx.accounts.token_vault_synthetic,
		&ctx.accounts.token_owner_account_synthetic,
		&ctx.accounts.token_program_synthetic,
		&ctx.accounts.memo_program,
		&remaining_accounts.transfer_hook_a,
		delta_synthetic,
		transfer_memo::TRANSFER_MEMO_DECREASE_LIQUIDITY.as_bytes()
	)?;

	transfer_from_vault_to_owner_v2(
		&ctx.accounts.amm,
		&ctx.accounts.token_mint_quote,
		&ctx.accounts.token_vault_quote,
		&ctx.accounts.token_owner_account_quote,
		&ctx.accounts.token_program_quote,
		&ctx.accounts.memo_program,
		&remaining_accounts.transfer_hook_b,
		delta_quote,
		transfer_memo::TRANSFER_MEMO_DECREASE_LIQUIDITY.as_bytes()
	)?;

	Ok(())
}
