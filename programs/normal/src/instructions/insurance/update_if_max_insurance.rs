use anchor_lang::prelude::*;

use crate::load_mut;

use super::initialize_insurance_fund::AdminUpdateInsurnaceFund;

pub fn handle_update_if_max_insurance(
	ctx: Context<AdminUpdateInsurnaceFund>,
	max_insurance: u64
) -> Result<()> {
	let insurance_fund = &mut load_mut!(ctx.accounts.insurance_fund)?;

	msg!("updating insurance fund {} IF max insurance");
	msg!(
		"insurance_fund.max_insurance: {:?} -> {:?}",
		insurance_fund.max_insurance,
		max_insurance
	);

	insurance_fund.max_insurance = max_insurance;
	Ok(())
}
