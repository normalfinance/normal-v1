use anchor_lang::prelude::*;

use crate::{ controller, load_mut, state::user::User };

#[derive(Accounts)]
pub struct CreateSchedule<'info> {
	pub state: Box<Account<'info, State>>,
	#[account(
        mut,
        constraint = can_sign_for_user(&user, &authority)?
    )]
	pub user: AccountLoader<'info, User>,
	pub authority: Signer<'info>,
}

#[access_control(exchange_not_paused(&ctx.accounts.state))]
pub fn handle_create_schedule<'c: 'info, 'info>(
	ctx: Context<'_, '_, 'c, 'info, CreateSchedule>,
	params: ScheduleParams
) -> Result<()> {
	let clock = &Clock::get()?;
	// let state = &ctx.accounts.state;

	let AccountMaps { perp_market_map, spot_market_map, mut oracle_map } =
		load_maps(
			&mut ctx.remaining_accounts.iter().peekable(),
			&MarketSet::new(),
			&MarketSet::new(),
			clock.slot,
			Some(state.oracle_guard_rails)
		)?;

	if params.immediate_or_cancel {
		msg!(
			"immediate_or_cancel order must be in place_and_make or place_and_take"
		);
		return Err(print_error!(ErrorCode::InvalidOrderIOC)().into());
	}

	let user_key = ctx.accounts.user.key();
	let mut user = load_mut!(ctx.accounts.user)?;

	controller::schedule::create_schedule(
		&ctx.accounts.state,
		&mut user,
		user_key,
		&perp_market_map,
		&spot_market_map,
		&mut oracle_map,
		clock,
		params,
		PlaceOrderOptions::default()
	)?;

	Ok(())
}
