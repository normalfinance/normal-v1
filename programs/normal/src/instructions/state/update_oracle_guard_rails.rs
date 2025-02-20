use anchor_lang::prelude::*;

use crate::state::state::State;

#[derive(Accounts)]
pub struct AdminUpdateState<'info> {
	pub admin: Signer<'info>,
	#[account(
        mut,
        has_one = admin
    )]
	pub state: Box<Account<'info, State>>,
}

pub fn handle_update_oracle_guard_rails(
	ctx: Context<AdminUpdateState>,
	oracle_guard_rails: OracleGuardRails
) -> Result<()> {
	msg!(
		"oracle_guard_rails: {:?} -> {:?}",
		ctx.accounts.state.oracle_guard_rails,
		oracle_guard_rails
	);

	ctx.accounts.state.oracle_guard_rails = oracle_guard_rails;
	Ok(())
}
