use anchor_lang::prelude::*;

use crate::{
	controller,
	instructions::optional_accounts::load_maps,
	load_mut,
	state::user::User,
	State,
};

#[derive(Accounts)]
pub struct SetUserStatusToBeingLiquidated<'info> {
	pub state: Box<Account<'info, State>>,
	#[account(mut)]
	pub user: AccountLoader<'info, User>,
	pub authority: Signer<'info>,
}

#[access_control(liq_not_paused(&ctx.accounts.state))]
pub fn handle_set_user_status_to_being_liquidated<'c: 'info, 'info>(
	ctx: Context<'_, '_, 'c, 'info, SetUserStatusToBeingLiquidated<'info>>
) -> Result<()> {
	let state = &ctx.accounts.state;
	let clock = Clock::get()?;
	let user = &mut load_mut!(ctx.accounts.user)?;

	let AccountMaps { market_map, mut oracle_map } = load_maps(
		&mut ctx.remaining_accounts.iter().peekable(),
		&MarketSet::new(),
		clock.slot,
		Some(state.oracle_guard_rails)
	)?;

	controller::liquidation::set_user_status_to_being_liquidated(
		user,
		&perp_market_map,
		&mut oracle_map,
		clock.slot,
		&state
	)?;

	Ok(())
}
