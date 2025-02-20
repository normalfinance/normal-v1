use anchor_lang::prelude::*;
use anchor_spl::token_interface::{ TokenAccount, TokenInterface };
use solana_program::instruction::Instruction;
use solana_program::sysvar::instructions::{
	load_current_index_checked,
	load_instruction_at_checked,
	ID as IX_ID,
};

use crate::state::market::Market;
use crate::{load_mut, validate, State};

#[derive(Accounts)]
#[instruction(market_index: u16,)]
pub struct SettleRevenueToInsuranceFund<'info> {
	pub state: Box<Account<'info, State>>,
	#[account(
        mut,
        seeds = [b"market", market_index.to_le_bytes().as_ref()],
        bump
    )]
	pub market: AccountLoader<'info, Market>,
	#[account(
        mut,
        seeds = [b"market_vault".as_ref(), market_index.to_le_bytes().as_ref()],
        bump,
    )]
	pub market_vault: Box<InterfaceAccount<'info, TokenAccount>>,
	#[account(constraint = state.signer.eq(&normal_signer.key()))]
	/// CHECK: forced normal_signer
	pub normal_signer: AccountInfo<'info>,
	#[account(
        mut,
        seeds = [b"insurance_fund_vault".as_ref(), market_index.to_le_bytes().as_ref()],
        bump,
    )]
	pub insurance_fund_vault: Box<InterfaceAccount<'info, TokenAccount>>,
	pub token_program: Interface<'info, TokenInterface>,
}

#[access_control(withdraw_not_paused(&ctx.accounts.state))]
pub fn handle_settle_revenue_to_if<'c: 'info, 'info>(
	ctx: Context<'_, '_, 'c, 'info, SettleRevenueToInsuranceFund<'info>>,
	market_index: u16
) -> Result<()> {
	let state = &ctx.accounts.state;
	let market = &mut load_mut!(ctx.accounts.market)?;

	let remaining_accounts_iter = &mut ctx.remaining_accounts.iter().peekable();
	let mint = get_token_mint(remaining_accounts_iter)?;

	validate!(
		market_index == market.market_index,
		ErrorCode::InvalidSpotMarketAccount,
		"invalid market passed"
	)?;

	validate!(
		insurance_fund.revenue_settle_period > 0,
		ErrorCode::RevenueSettingsCannotSettleToIF,
		"invalid revenue_settle_period settings on market"
	)?;

	let market_vault_amount = ctx.accounts.market_vault.amount;
	let insurance_vault_amount = ctx.accounts.insurance_fund_vault.amount;

	let clock = Clock::get()?;
	let now = clock.unix_timestamp;

	let time_until_next_update = math::helpers::on_the_hour_update(
		now,
		insurance_fund.last_revenue_settle_ts,
		insurance_fund.revenue_settle_period
	)?;

	validate!(
		time_until_next_update == 0,
		ErrorCode::RevenueSettingsCannotSettleToIF,
		"Must wait {} seconds until next available settlement time",
		time_until_next_update
	)?;

	// uses proportion of revenue pool allocated to insurance fund
	let token_amount = controller::insurance::settle_revenue_to_insurance_fund(
		spot_vault_amount,
		insurance_vault_amount,
		market,
		now,
		true
	)?;

	insurance_fund.last_revenue_settle_ts = now;

	controller::token::send_from_program_vault(
		&ctx.accounts.token_program,
		&ctx.accounts.spot_market_vault,
		&ctx.accounts.insurance_fund_vault,
		&ctx.accounts.normal_signer,
		state.signer_nonce,
		token_amount,
		&mint
	)?;

	// reload the spot market vault balance so it's up-to-date
	ctx.accounts.market_vault.reload()?;
	math::spot_withdraw::validate_spot_market_vault_amount(
		spot_market,
		ctx.accounts.spot_market_vault.amount
	)?;

	Ok(())
}
