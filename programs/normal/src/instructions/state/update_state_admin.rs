use anchor_lang::prelude::*;

use super::AdminUpdateState;

pub fn handle_update_state_admin(
	ctx: Context<AdminUpdateState>,
	admin: Pubkey
) -> Result<()> {
	msg!("admin: {:?} -> {:?}", ctx.accounts.state.admin, admin);
	ctx.accounts.state.admin = admin;
	Ok(())
}
