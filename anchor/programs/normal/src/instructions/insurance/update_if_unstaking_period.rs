use crate::state::*;
use anchor_lang::prelude::*;
use anchor_spl::token::{ self, Mint, Token, TokenAccount };
use insurance::InsuranceFund;


pub fn handle_set_if_unstaking_period(
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
