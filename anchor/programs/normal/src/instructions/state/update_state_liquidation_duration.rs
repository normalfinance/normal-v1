use anchor_lang::prelude::*;

use super::AdminUpdateState;

pub fn handle_update_state_liquidation_duration(
	ctx: Context<AdminUpdateState>,
	liquidation_duration: u8
) -> Result<()> {
	msg!(
		"liquidation_duration: {} -> {}",
		ctx.accounts.state.liquidation_duration,
		liquidation_duration
	);

	ctx.accounts.state.liquidation_duration = liquidation_duration;
	Ok(())
}
