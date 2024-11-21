use anchor_lang::prelude::*;

use crate::State;

#[derive(Accounts)]
pub struct AdminUpdateState<'info> {
	pub admin: Signer<'info>,
	#[account(
        mut,
        has_one = admin
    )]
	pub state: Box<Account<'info, State>>,
}

pub fn handle_add_collateral_type(
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
