use anchor_lang::prelude::*;
use anchor_spl::token_interface::{ TokenAccount, TokenInterface };

use crate::error::ErrorCode;
use crate::instructions::constraints::*;
use crate::optional_accounts::get_token_mint;
use crate::state::insurance_fund_stake::InsuranceFundStake;
use crate::state::paused_operations::InsuranceFundOperation;
use crate::state::state::State;
use crate::state::traits::Size;
use crate::validate;
use crate::{ controller, math };
use crate::load_mut;

use super::request_remove_insurance_fund_stake::RequestRemoveInsuranceFundStake;

pub fn handle_cancel_request_remove_insurance_fund_stake(
	ctx: Context<RequestRemoveInsuranceFundStake>
) -> Result<()> {
	let clock = Clock::get()?;
	let now = clock.unix_timestamp;
	let insurance_fund_stake = &mut load_mut!(ctx.accounts.insurance_fund_stake)?;
	let insurance_fund = &mut load_mut!(ctx.accounts.insurance_fund)?;

	validate!(
		insurance_fund_stake.last_withdraw_request_shares != 0,
		ErrorCode::NoIFWithdrawRequestInProgress,
		"No withdraw request in progress"
	)?;

	controller::insurance::cancel_request_remove_insurance_fund_stake(
		ctx.accounts.insurance_fund_vault.amount,
		insurance_fund_stake,
		user_stats,
		spot_market,
		now
	)?;

	Ok(())
}
