use anchor_lang::prelude::*;

use super::AdminUpdateState;

pub fn handle_update_state_liquidation_margin_buffer_ratio(
	ctx: Context<AdminUpdateState>,
	liquidation_margin_buffer_ratio: u32
) -> Result<()> {
	msg!(
		"liquidation_margin_buffer_ratio: {} -> {}",
		ctx.accounts.state.liquidation_margin_buffer_ratio,
		liquidation_margin_buffer_ratio
	);

	ctx.accounts.state.liquidation_margin_buffer_ratio =
		liquidation_margin_buffer_ratio;
	Ok(())
}
