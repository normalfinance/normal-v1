use anchor_lang::prelude::*;

use super::update_oracle_guard_rails::AdminUpdateState;

pub fn handle_update_state_initial_pct_to_liquidate(
	ctx: Context<AdminUpdateState>,
	initial_pct_to_liquidate: u16
) -> Result<()> {
	msg!(
		"initial_pct_to_liquidate: {} -> {}",
		ctx.accounts.state.initial_pct_to_liquidate,
		initial_pct_to_liquidate
	);

	ctx.accounts.state.initial_pct_to_liquidate = initial_pct_to_liquidate;
	Ok(())
}
