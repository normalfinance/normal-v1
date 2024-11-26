use anchor_lang::prelude::*;

use super::update_state_initial_pct_to_liquidate::AdminUpdateState;

pub fn handle_update_state_admin(
	ctx: Context<AdminUpdateState>,
	admin: Pubkey
) -> Result<()> {
	msg!("admin: {:?} -> {:?}", ctx.accounts.state.admin, admin);
	ctx.accounts.state.admin = admin;
	Ok(())
}
