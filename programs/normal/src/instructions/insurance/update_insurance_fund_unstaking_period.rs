use crate::state::*;
use anchor_lang::prelude::*;
use anchor_spl::token::{ self, Mint, Token, TokenAccount };
use insurance::InsuranceFund;

#[derive(Accounts)]
pub struct AdminUpdateInsurnaceFund<'info> {
	pub admin: Signer<'info>,
	#[account(has_one = admin)]
	pub state: Box<Account<'info, State>>,
	#[account(mut)]
	pub insurance_fund: AccountLoader<'info, InsuranceFund>,
}

pub fn handle_set_insurance_fund_unstaking_period(
	ctx: Context<AdminUpdateInsurnaceFund>,
	unstaking_period: i64
) -> Result<()> {
	let insurance_fund = &mut load_mut!(ctx.accounts.insurance_fund)?;

	msg!("updating insurance fund {} IF unstaking period");
	msg!(
		"insurance_fund.unstaking_period: {:?} -> {:?}",
		insurance_fund.unstaking_period,
		insurance_fund_unstaking_period
	);

	insurance_fund.unstaking_period = insurance_fund_unstaking_period;
	Ok(())
}
