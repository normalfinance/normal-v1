use anchor_lang::prelude::*;

use crate::errors::ErrorCode;
use crate::instructions::constraints::*;
use crate::state::insurance::InsuranceFund;
use crate::state::insurance::InsuranceFundStake;
use crate::state::paused_operations::InsuranceFundOperation;
use crate::state::state::State;
use crate::state::user_stats::UserStats;
use crate::validate;
use crate::controller;
use crate::load_mut;

#[derive(Accounts)]
#[instruction(market_index: u16,)]
pub struct RemoveInsuranceFundStake<'info> {
	pub state: Box<Account<'info, State>>,
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
	#[account(
        mut,
        has_one = authority,
    )]
	pub user_stats: AccountLoader<'info, UserStats>,
	pub authority: Signer<'info>,
	#[account(
        mut,
        seeds = [b"insurance_fund_vault".as_ref()],
        bump,
    )]
	pub insurance_fund_vault: Box<InterfaceAccount<'info, TokenAccount>>,
	#[account(constraint = state.signer.eq(&normal_signer.key()))]
	/// CHECK: forced normal_signer
	pub normal_signer: AccountInfo<'info>,
	#[account(
        mut,
        token::mint = insurance_fund_vault.mint,
        token::authority = authority
    )]
	pub user_token_account: Box<InterfaceAccount<'info, TokenAccount>>,
	pub token_program: Interface<'info, TokenInterface>,
}

#[access_control(withdraw_not_paused(&ctx.accounts.state))]
pub fn handle_remove_insurance_fund_stake<'c: 'info, 'info>(
	ctx: Context<'_, '_, 'c, 'info, RemoveInsuranceFundStake<'info>>
) -> Result<()> {
	let clock = Clock::get()?;
	let now = clock.unix_timestamp;
	let insurance_fund_stake = &mut load_mut!(ctx.accounts.insurance_fund_stake)?;
	let user_stats = &mut load_mut!(ctx.accounts.user_stats)?;
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
		user_stats,
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
