use anchor_lang::prelude::*;

use crate::{ load_mut, state::paused_operations::InsuranceFundOperation };

use super::AdminUpdateInsurnaceFund;

pub fn handle_update_if_paused_operations(
	ctx: Context<AdminUpdateInsurnaceFund>,
	paused_operations: u8
) -> Result<()> {
	let insurance_fund = &mut load_mut!(ctx.accounts.insurance_fund)?;

	insurance_fund.paused_operations = EM;

	InsuranceFundOperation::log_all_operations_paused(
		insurance_fund.paused_operations
	);

	Ok(())
}
