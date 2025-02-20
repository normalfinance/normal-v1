use anchor_lang::prelude::*;

use crate::state::traits::Size;
use crate::{
	errors::ErrorCode,
	safe_increment,
	state::user_stats::UserStats,
	validate,
	State,
};
use crate::math_error;

#[derive(Accounts)]
pub struct InitializeUserStats<'info> {
	#[account(
		init,
		seeds = [b"user_stats", authority.key.as_ref()],
		space = UserStats::SIZE,
		bump,
		payer = payer
	)]
	pub user_stats: AccountLoader<'info, UserStats>,
	#[account(mut)]
	pub state: Box<Account<'info, State>>,
	pub authority: Signer<'info>,
	#[account(mut)]
	pub payer: Signer<'info>,
	pub rent: Sysvar<'info, Rent>,
	pub system_program: Program<'info, System>,
}

pub fn handle_initialize_user_stats<'c: 'info, 'info>(
	ctx: Context<'_, '_, 'c, 'info, InitializeUserStats>
) -> Result<()> {
	let clock = Clock::get()?;

	let mut user_stats = ctx.accounts.user_stats
		.load_init()
		.or(Err(ErrorCode::UnableToLoadAccountLoader))?;

	*user_stats = UserStats {
		authority: ctx.accounts.authority.key(),
		number_of_sub_accounts: 0,
		// last_taker_volume_30d_ts: clock.unix_timestamp,
		// last_maker_volume_30d_ts: clock.unix_timestamp,
		// last_filler_volume_30d_ts: clock.unix_timestamp,
		// last_fuel_if_bonus_update_ts: clock.unix_timestamp.cast()?,
		..UserStats::default()
	};

	let state = &mut ctx.accounts.state;
	safe_increment!(state.number_of_authorities, 1);

	let max_number_of_sub_accounts = state.max_number_of_sub_accounts();

	validate!(
		max_number_of_sub_accounts == 0 ||
			state.number_of_authorities <= max_number_of_sub_accounts,
		ErrorCode::MaxNumberOfUsers
	)?;

	Ok(())
}
