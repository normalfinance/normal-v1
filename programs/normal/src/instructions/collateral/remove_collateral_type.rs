use anchor_lang::prelude::*;

use crate::{ instructions::AdminUpdateState, State };

pub fn handle_remove_collateral_type(
	ctx: Context<AdminUpdateState>
) -> Result<()> {
	msg!(
		"collateral_types: {:?} -> {:?}",
		ctx.accounts.state.collateral_types,
		fee_structure
	);

	ctx.accounts.state.collateral_types = fee_structure;

	Ok(())
}
