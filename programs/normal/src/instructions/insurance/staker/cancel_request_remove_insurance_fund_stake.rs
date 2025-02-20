use anchor_lang::prelude::*;

use crate::errors::ErrorCode;
use crate::validate;
use crate::controller;
use crate::load_mut;

use super::request_remove_insurance_fund_stake::RequestRemoveInsuranceFundStake;

pub fn handle_cancel_request_remove_insurance_fund_stake(
	ctx: Context<RequestRemoveInsuranceFundStake>
) -> Result<()> {
	let clock = Clock::get()?;
	let now = clock.unix_timestamp;
	let insurance_fund_stake = &mut load_mut!(ctx.accounts.insurance_fund_stake)?;
	let user_stats = &mut load_mut!(ctx.accounts.user_stats)?;
	let insurance_fund = &mut load_mut!(ctx.accounts.insurance_fund)?;

	validate!(
		insurance_fund_stake.last_withdraw_request_shares != 0,
		ErrorCode::NoIFWithdrawRequestInProgress,
		"No withdraw request in progress"
	)?;

	controller::insurance::cancel_request_remove_insurance_fund_stake(
		ctx.accounts.insurance_fund_vault.amount,
		insurance_fund_stake,
		insurance_fund,
		user_stats,
		now
	)?;

	Ok(())
}
