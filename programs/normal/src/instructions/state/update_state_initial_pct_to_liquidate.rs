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
