use anchor_lang::prelude::*;

use super::AdminUpdateState;

pub fn handle_update_state_max_number_of_sub_accounts(
	ctx: Context<AdminUpdateState>,
	max_number_of_sub_accounts: u16
) -> Result<()> {
	msg!(
		"max_number_of_sub_accounts: {} -> {}",
		ctx.accounts.state.max_number_of_sub_accounts,
		max_number_of_sub_accounts
	);

	ctx.accounts.state.max_number_of_sub_accounts = max_number_of_sub_accounts;
	Ok(())
}
