use anchor_lang::prelude::*;

use crate::load_mut;

use super::initialize_insurance_fund::AdminUpdateInsurnaceFund;

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
