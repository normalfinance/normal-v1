use anchor_lang::prelude::*;

use super::update_oracle_guard_rails::AdminUpdateState;

pub fn handle_update_exchange_status(
	ctx: Context<AdminUpdateState>,
	exchange_status: u8
) -> Result<()> {
	msg!(
		"exchange_status: {:?} -> {:?}",
		ctx.accounts.state.exchange_status,
		exchange_status
	);

	ctx.accounts.state.exchange_status = exchange_status;
	Ok(())
}
