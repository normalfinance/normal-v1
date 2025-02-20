use anchor_lang::prelude::*;

use crate::{
	instructions::{
		can_sign_for_user,
		optional_accounts::{ load_maps, AccountMaps },
	},
	load_mut,
	math::constants::QUOTE_PRECISION_I128,
	state::{ state::State, user::User },
	validation::user::validate_user_is_idle,
};
use crate::instructions::constraints::*;

#[derive(Accounts)]
pub struct UpdateUserIdle<'info> {
	pub state: Box<Account<'info, State>>,
	pub authority: Signer<'info>,
	#[account(
        mut,
        constraint = can_sign_for_user(&filler, &authority)?
    )]
	pub filler: AccountLoader<'info, User>,
	#[account(mut)]
	pub user: AccountLoader<'info, User>,
}

#[access_control(exchange_not_paused(&ctx.accounts.state))]
pub fn handle_update_user_idle<'c: 'info, 'info>(
	ctx: Context<'_, '_, 'c, 'info, UpdateUserIdle<'info>>
) -> Result<()> {
	let mut user = load_mut!(ctx.accounts.user)?;
	let clock = Clock::get()?;

	let AccountMaps {
		// market_map,
		mut oracle_map,
	} = load_maps(
		&mut ctx.remaining_accounts.iter().peekable(),
		// &MarketSet::new(),
		Clock::get()?.slot,
		None
	)?;

	// let (equity, _) = calculate_user_equity(
	// 	&user,
	// 	&market_map,
	// 	&mut oracle_map
	// )?;

	// // user flipped to idle faster if equity is less than 1000
	// let accelerated = equity < QUOTE_PRECISION_I128 * 1000;

	// validate_user_is_idle(&user, clock.slot, accelerated)?;

	user.idle = true;

	Ok(())
}
