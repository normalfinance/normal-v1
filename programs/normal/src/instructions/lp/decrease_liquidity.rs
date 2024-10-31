use anchor_lang::prelude::*;

use crate::controller;
use crate::errors::ErrorCode;
use crate::manager::liquidity_manager::{
	calculate_liquidity_token_deltas,
	calculate_modify_liquidity,
	sync_modify_liquidity_values,
};
use crate::math::{ self, convert_to_liquidity_delta };
use crate::util::{
	burn_synthetic_from_vault,
	to_timestamp_u64,
	transfer_from_vault_to_owner,
	verify_position_authority_interface,
};

use super::increase_liquidity::ModifyLiquidity;

/*
  Removes liquidity from an existing AMM Position.
*/
pub fn handle_decrease_liquidity(
	ctx: Context<ModifyLiquidity>,
	liquidity_amount: u128,
	token_min_b: u64
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

	if delta_b < token_min_b {
		return Err(ErrorCode::TokenMinSubceeded.into());
	}

	// Burn a delta_a amount of synthetic tokens from the AMM
	burn_synthetic_from_vault(
		authority,
		token_owner_account,
		token_vault,
		token_program,
		amount
	)?;
	// transfer_from_vault_to_owner(
	// 	&ctx.accounts.amm,
	// 	&ctx.accounts.token_vault_a,
	// 	&ctx.accounts.token_owner_account_a,
	// 	&ctx.accounts.token_program,
	// 	delta_a
	// )?;

	transfer_from_vault_to_owner(
		&ctx.accounts.amm,
		&ctx.accounts.token_vault_b,
		&ctx.accounts.token_owner_account_b,
		&ctx.accounts.token_program,
		delta_b
	)?;

	Ok(())
}
