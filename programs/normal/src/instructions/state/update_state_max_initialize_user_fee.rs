use anchor_lang::prelude::*;

use super::update_oracle_guard_rails::AdminUpdateState;

pub fn handle_update_state_max_initialize_user_fee(
	ctx: Context<AdminUpdateState>,
	max_initialize_user_fee: u16
) -> Result<()> {
	msg!(
		"max_initialize_user_fee: {} -> {}",
		ctx.accounts.state.max_initialize_user_fee,
		max_initialize_user_fee
	);

	ctx.accounts.state.max_initialize_user_fee = max_initialize_user_fee;
	Ok(())
}
