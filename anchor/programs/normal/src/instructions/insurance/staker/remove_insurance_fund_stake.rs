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

#[access_control(withdraw_not_paused(&ctx.accounts.state))]
pub fn handle_remove_insurance_fund_stake<'c: 'info, 'info>(
	ctx: Context<'_, '_, 'c, 'info, RequestRemoveInsuranceFundStake<'info>>
) -> Result<()> {
	let clock = Clock::get()?;
	let now = clock.unix_timestamp;
	let insurance_fund_stake = &mut load_mut!(ctx.accounts.insurance_fund_stake)?;
	let insurance_fund = &mut load_mut!(ctx.accounts.insurance_fund)?;
	let state = &ctx.accounts.state;

	let remaining_accounts_iter = &mut ctx.remaining_accounts.iter().peekable();
	let mint = get_token_mint(remaining_accounts_iter)?;

	validate!(
		!insurance_fund.is_operation_paused(InsuranceFundOperation::Remove),
		ErrorCode::InsuranceFundOperationPaused,
		"if staking remove disabled"
	)?;

	// TODO: check insurnace utilization?

	let amount = controller::insurance::remove_insurance_fund_stake(
		ctx.accounts.insurance_fund_vault.amount,
		insurance_fund_stake,
		insurance_fund,
		now
	)?;

	controller::token::send_from_program_vault(
		&ctx.accounts.token_program,
		&ctx.accounts.insurance_fund_vault,
		&ctx.accounts.user_token_account,
		&ctx.accounts.normal_signer,
		state.signer_nonce,
		amount,
		&mint
	)?;

	ctx.accounts.insurance_fund_vault.reload()?;
	validate!(
		ctx.accounts.insurance_fund_vault.amount > 0,
		ErrorCode::InvalidIFDetected,
		"insurance_fund_vault.amount must remain > 0"
	)?;

	// validate relevant spot market balances before unstake
	// math::spot_withdraw::validate_spot_balances(spot_market)?;

	Ok(())
}
