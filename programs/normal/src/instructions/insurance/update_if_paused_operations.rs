use anchor_lang::prelude::*;

use crate::{ load_mut, state::paused_operations::InsuranceFundOperation };

use super::initialize_insurance_fund::AdminUpdateInsurnaceFund;

pub fn handle_update_if_paused_operations(
	ctx: Context<AdminUpdateInsurnaceFund>,
	paused_operations: u8
) -> Result<()> {
	let insurance_fund = &mut load_mut!(ctx.accounts.insurance_fund)?;

	insurance_fund.paused_operations = paused_operations;

	InsuranceFundOperation::log_all_operations_paused(
		insurance_fund.paused_operations
	);

	Ok(())
}
