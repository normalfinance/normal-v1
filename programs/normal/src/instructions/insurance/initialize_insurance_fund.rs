use anchor_lang::prelude::*;

use crate::{
	errors::ErrorCode,
	state::{ insurance::InsuranceFund, state::State },
	validate,
};

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

#[derive(Accounts)]
pub struct AdminUpdateInsurnaceFund<'info> {
	pub admin: Signer<'info>,
	#[account(has_one = admin)]
	pub state: Box<Account<'info, State>>,
	#[account(mut)]
	pub insurance_fund: AccountLoader<'info, InsuranceFund>,
}
