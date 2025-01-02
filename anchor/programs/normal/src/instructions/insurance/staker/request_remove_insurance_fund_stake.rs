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

#[derive(Accounts)]
pub struct RequestRemoveInsuranceFundStake<'info> {
	#[account(
        mut,
        seeds = [b"insurance_fund"],
        bump
    )]
	pub insurance_fund: AccountLoader<'info, InsuranceFund>,
	#[account(
        mut,
        has_one = authority,
    )]
	pub insurance_fund_stake: AccountLoader<'info, InsuranceFundStake>,
	pub authority: Signer<'info>,
	#[account(
        mut,
        seeds = [b"insurance_fund_vault".as_ref()],
        bump,
    )]
	pub insurance_fund_vault: Box<InterfaceAccount<'info, TokenAccount>>,
}

pub fn handle_request_remove_insurance_fund_stake(
	ctx: Context<RequestRemoveInsuranceFundStake>,
	amount: u64
) -> Result<()> {
	let clock = Clock::get()?;
	let insurance_fund_stake = &mut load_mut!(ctx.accounts.insurance_fund_stake)?;
	let insurance_fund = &mut load_mut!(ctx.accounts.insurance_fund)?;

	validate!(
		!insurance_fund.is_operation_paused(InsuranceFundOperation::RequestRemove),
		ErrorCode::InsuranceFundOperationPaused,
		"if staking request remove disabled"
	)?;

	validate!(
		insurance_fund_stake.last_withdraw_request_shares == 0,
		ErrorCode::IFWithdrawRequestInProgress,
		"Withdraw request is already in progress"
	)?;

	let n_shares = math::insurance::vault_amount_to_if_shares(
		amount,
		insurance_fund.total_shares,
		ctx.accounts.insurance_fund_vault.amount
	)?;

	validate!(
		n_shares > 0,
		ErrorCode::IFWithdrawRequestTooSmall,
		"Requested lp_shares = 0"
	)?;

	let user_if_shares = insurance_fund_stake.checked_if_shares(insurance_fund)?;
	validate!(user_if_shares >= n_shares, ErrorCode::InsufficientIFShares)?;

	controller::insurance::request_remove_insurance_fund_stake(
		n_shares,
		ctx.accounts.insurance_fund_vault.amount,
		insurance_fund_stake,
		insurance_fund,
		clock.unix_timestamp
	)?;

	Ok(())
}
