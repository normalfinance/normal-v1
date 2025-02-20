use anchor_lang::prelude::*;

use crate::errors::ErrorCode;
use crate::state::paused_operations::InsuranceFundOperation;
use crate::state::state::State;
use crate::validate;

#[derive(Accounts)]
pub struct InitializeInsuranceFundStake<'info> {
	#[account(seeds = [b"insurance_fund"], bump)]
	pub insurance_fund: AccountLoader<'info, InsuranceFund>,
	#[account(
		init,
		seeds = [b"insurance_fund_stake", authority.key.as_ref()],
		space = InsuranceFundStake::SIZE,
		bump,
		payer = payer
	)]
	pub insurance_fund_stake: AccountLoader<'info, InsuranceFundStake>,
	pub state: Box<Account<'info, State>>,
	pub authority: Signer<'info>,
	#[account(mut)]
	pub payer: Signer<'info>,
	pub rent: Sysvar<'info, Rent>,
	pub system_program: Program<'info, System>,
}

pub fn handle_initialize_insurance_fund_stake(
	ctx: Context<InitializeInsuranceFundStake>
) -> Result<()> {
	let mut if_stake = ctx.accounts.insurance_fund_stake
		.load_init()
		.or(Err(ErrorCode::UnableToLoadAccountLoader))?;

	let clock = Clock::get()?;
	let now = clock.unix_timestamp;

	*if_stake = InsuranceFundStake::new(*ctx.accounts.authority.key, now);

	let insurance_fund = ctx.accounts.insurance_fund.load()?;

	validate!(
		!insurance_fund.is_operation_paused(InsuranceFundOperation::Init),
		ErrorCode::InsuranceFundOperationPaused,
		"if staking init disabled"
	)?;

	Ok(())
}
