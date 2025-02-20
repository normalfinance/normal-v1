#![allow(clippy::too_many_arguments)]
#![allow(clippy::bool_assert_comparison)]
#![allow(clippy::comparison_chain)]

use anchor_lang::prelude::*;

use crate::state::state::*;

pub mod controller;
pub mod errors;
pub mod ids;
pub mod instructions;
pub mod macros;
pub mod math;
mod signer;
pub mod security;
pub mod state;

use instructions::*;

#[cfg(test)]
mod test_utils;
mod validation;

declare_id!("BqxukimGxeWzUJSwpDyGoc6Q78iMtAhwSYxgiq2vXTxB");

#[program]
pub mod normal {
	use super::*;

	// State instructions

	pub fn initialize(
		ctx: Context<Initialize>,
		total_debt_ceiling: u64
	) -> Result<()> {
		handle_initialize_state(ctx, total_debt_ceiling)
	}

	pub fn update_state_admin(
		ctx: Context<AdminUpdateState>,
		admin: Pubkey
	) -> Result<()> {
		handle_update_state_admin(ctx, admin)
	}

	pub fn update_state_initial_pct_to_liquidate(
		ctx: Context<AdminUpdateState>,
		initial_pct_to_liquidate: u16
	) -> Result<()> {
		handle_update_state_initial_pct_to_liquidate(ctx, initial_pct_to_liquidate)
	}

	pub fn update_state_liquidation_duration(
		ctx: Context<AdminUpdateState>,
		liquidation_duration: u8
	) -> Result<()> {
		handle_update_state_liquidation_duration(ctx, liquidation_duration)
	}

	pub fn update_state_liquidation_margin_buffer_ratio(
		ctx: Context<AdminUpdateState>,
		liquidation_margin_buffer_ratio: u32
	) -> Result<()> {
		handle_update_state_liquidation_margin_buffer_ratio(
			ctx,
			liquidation_margin_buffer_ratio
		)
	}

	pub fn update_state_max_number_of_sub_accounts(
		ctx: Context<AdminUpdateState>,
		max_number_of_sub_accounts: u16
	) -> Result<()> {
		handle_update_state_max_number_of_sub_accounts(
			ctx,
			max_number_of_sub_accounts
		)
	}

	pub fn update_state_max_initialize_user_fee(
		ctx: Context<AdminUpdateState>,
		max_initialize_user_fee: u16
	) -> Result<()> {
		handle_update_state_max_initialize_user_fee(ctx, max_initialize_user_fee)
	}

	// User instructions

	pub fn initialize_user<'c: 'info, 'info>(
		ctx: Context<'_, '_, 'c, 'info, InitializeUser<'info>>,
		sub_account_id: u16,
		name: [u8; 32]
	) -> Result<()> {
		handle_initialize_user(ctx, sub_account_id, name)
	}

	pub fn initialize_user_stats<'c: 'info, 'info>(
		ctx: Context<'_, '_, 'c, 'info, InitializeUserStats>
	) -> Result<()> {
		handle_initialize_user_stats(ctx)
	}

	pub fn initialize_referrer_name(
		ctx: Context<InitializeReferrerName>,
		name: [u8; 32]
	) -> Result<()> {
		handle_initialize_referrer_name(ctx, name)
	}

	pub fn update_user_name(
		ctx: Context<UpdateUser>,
		_sub_account_id: u16,
		name: [u8; 32]
	) -> Result<()> {
		handle_update_user_name(ctx, _sub_account_id, name)
	}

	pub fn update_user_delegate(
		ctx: Context<UpdateUser>,
		_sub_account_id: u16,
		delegate: Pubkey
	) -> Result<()> {
		handle_update_user_delegate(ctx, _sub_account_id, delegate)
	}

	pub fn update_user_idle<'c: 'info, 'info>(
		ctx: Context<'_, '_, 'c, 'info, UpdateUserIdle<'info>>
	) -> Result<()> {
		handle_update_user_idle(ctx)
	}

	pub fn update_user_reduce_only(
		ctx: Context<UpdateUser>,
		_sub_account_id: u16,
		reduce_only: bool
	) -> Result<()> {
		handle_update_user_reduce_only(ctx, _sub_account_id, reduce_only)
	}

	pub fn update_user_custom_margin_ratio(
		ctx: Context<UpdateUser>,
		_sub_account_id: u16,
		margin_ratio: u32
	) -> Result<()> {
		handle_update_user_custom_margin_ratio(ctx, _sub_account_id, margin_ratio)
	}

	pub fn delete_user<'c: 'info, 'info>(
		ctx: Context<'_, '_, 'c, 'info, DeleteUser>
	) -> Result<()> {
		handle_delete_user(ctx)
	}

	pub fn reclaim_rent(ctx: Context<ReclaimRent>) -> Result<()> {
		handle_reclaim_rent(ctx)
	}

	// Insurance Fund instructions

	pub fn initialize_insurance_fund(
		ctx: Context<InitializeInsuranceFund>,
		if_total_factor: u32
	) -> Result<()> {
		handle_initialize_insurance_fund(ctx, if_total_factor)
	}

	pub fn update_if_max_insurance(
		ctx: Context<AdminUpdateInsurnaceFund>,
		max_insurance: u64
	) -> Result<()> {
		handle_update_if_max_insurance(ctx, max_insurance)
	}

	pub fn update_if_paused_operations(
		ctx: Context<AdminUpdateInsurnaceFund>,
		paused_operations: u8
	) -> Result<()> {
		handle_update_if_paused_operations(ctx, paused_operations)
	}

	pub fn update_if_unstaking_period(
		ctx: Context<AdminUpdateInsurnaceFund>,
		if_unstaking_period: i64
	) -> Result<()> {
		handle_update_if_unstaking_period(ctx, if_unstaking_period)
	}

	// Insurane Fund Staker instructions

	pub fn initialize_insurance_fund_stake(
		ctx: Context<InitializeInsuranceFundStake>
	) -> Result<()> {
		handle_initialize_insurance_fund_stake(ctx)
	}

	pub fn add_insurance_fund_stake<'c: 'info, 'info>(
		ctx: Context<'_, '_, 'c, 'info, AddInsuranceFundStake<'info>>,
		amount: u64
	) -> Result<()> {
		handle_add_insurance_fund_stake(ctx, amount)
	}

	pub fn request_remove_insurance_fund_stake(
		ctx: Context<RequestRemoveInsuranceFundStake>,
		amount: u64
	) -> Result<()> {
		handle_request_remove_insurance_fund_stake(ctx, amount)
	}

	pub fn cancel_request_remove_insurance_fund_stake(
		ctx: Context<RequestRemoveInsuranceFundStake>
	) -> Result<()> {
		handle_cancel_request_remove_insurance_fund_stake(ctx)
	}

	pub fn remove_insurance_fund_stake<'c: 'info, 'info>(
		ctx: Context<'_, '_, 'c, 'info, RemoveInsuranceFundStake<'info>>
	) -> Result<()> {
		handle_remove_insurance_fund_stake(ctx)
	}
}
