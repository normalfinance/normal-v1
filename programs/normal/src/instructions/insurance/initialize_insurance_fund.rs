use crate::state::*;
use anchor_lang::prelude::*;
use anchor_spl::token::{ self, Mint, Token, TokenAccount };
use insurance::InsuranceFund;

#[derive(Accounts)]
pub struct InitializeInsuranceFund<'info> {
	#[account(seeds = [b"insurance_fund"], bump)]
	pub insurance_fund: AccountLoader<'info, InsuranceFund>,

	#[account(mut)]
	pub payer: Signer<'info>,
	pub rent: Sysvar<'info, Rent>,
	pub system_program: Program<'info, System>,
}

pub fn handle_initialize_insurance_fund(
	ctx: Context<InitializeInsuranceFund>
) -> Result<()> {
	let mut insurance_fund = ctx.accounts.insurance_fund
		.load_init()
		.or(Err(ErrorCode::UnableToLoadAccountLoader))?;

	let clock = Clock::get()?;
	let now = clock.unix_timestamp;

	*insurance_fund = InsuranceFund::new(*ctx.accounts.authority.key, now);

	let insurance_fund = ctx.accounts.insurance_fund.load()?;

	validate!(
		!insurance_fund.is_operation_paused(InsuranceFundOperation::Init),
		ErrorCode::InsuranceFundOperationPaused,
		"if staking init disabled"
	)?;

	Ok(())
}
