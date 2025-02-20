use anchor_lang::prelude::*;

use crate::{
	load,
	load_mut,
	safe_decrement,
	state::{ state::State, user::User, user_stats::UserStats },
};

#[derive(Accounts)]
pub struct DeleteUser<'info> {
	#[account(
        mut,
        has_one = authority,
        close = authority
    )]
	pub user: AccountLoader<'info, User>,
	#[account(
        mut,
        has_one = authority
    )]
	pub user_stats: AccountLoader<'info, UserStats>,
	#[account(mut)]
	pub state: Box<Account<'info, State>>,
	pub authority: Signer<'info>,
}

pub fn handle_delete_user(ctx: Context<DeleteUser>) -> Result<()> {
	let user = &load!(ctx.accounts.user)?;
	let user_stats = &mut load_mut!(ctx.accounts.user_stats)?;

	validate_user_deletion(
		user,
		user_stats,
		&ctx.accounts.state,
		Clock::get()?.unix_timestamp
	)?;

	safe_decrement!(user_stats.number_of_sub_accounts, 1);

	let state = &mut ctx.accounts.state;
	safe_decrement!(state.number_of_sub_accounts, 1);

	Ok(())
}
