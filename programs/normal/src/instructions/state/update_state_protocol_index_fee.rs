use anchor_lang::prelude::*;

use super::AdminUpdateState;

pub fn handle_update_state_protocol_index_fee(
	ctx: Context<AdminUpdateState>,
	protocol_index_fee: u16
) -> Result<()> {
	msg!(
		"protocol_index_fee: {} -> {}",
		ctx.accounts.state.protocol_index_fee,
		protocol_index_fee
	);

	ctx.accounts.state.protocol_index_fee = protocol_index_fee;
	Ok(())
}
