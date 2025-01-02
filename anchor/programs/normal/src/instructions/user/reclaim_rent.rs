use anchor_lang::prelude::*;
use anchor_lang::Discriminator;

use crate::state::user::ReferrerName;
use crate::state::user::User;
use crate::state::user::UserStats;
use crate::State;

#[derive(Accounts)]
pub struct ReclaimRent<'info> {
	#[account(
        mut,
        has_one = authority,
    )]
	pub user: AccountLoader<'info, User>,
	#[account(
        mut,
        has_one = authority
    )]
	pub user_stats: AccountLoader<'info, UserStats>,
	pub state: Box<Account<'info, State>>,
	pub authority: Signer<'info>,
	pub rent: Sysvar<'info, Rent>,
}

pub fn handle_reclaim_rent(ctx: Context<ReclaimRent>) -> Result<()> {
	let user_size = ctx.accounts.user.to_account_info().data_len();
	let minimum_lamports = ctx.accounts.rent.minimum_balance(user_size);
	let current_lamports = ctx.accounts.user.to_account_info().try_lamports()?;
	let reclaim_amount = current_lamports.saturating_sub(minimum_lamports);

	validate!(
		reclaim_amount > 0,
		ErrorCode::CantReclaimRent,
		"user account has no excess lamports to reclaim"
	)?;

	**ctx.accounts.user.to_account_info().try_borrow_mut_lamports()? =
		minimum_lamports;

	**ctx.accounts.authority.to_account_info().try_borrow_mut_lamports()? +=
		reclaim_amount;

	let user_stats = &mut load!(ctx.accounts.user_stats)?;

	// Skip age check if is no max sub accounts
	let max_sub_accounts = ctx.accounts.state.max_number_of_sub_accounts();
	let estimated_user_stats_age = user_stats.get_age_ts(
		Clock::get()?.unix_timestamp
	);
	validate!(
		max_sub_accounts == 0 || estimated_user_stats_age >= THIRTEEN_DAY,
		ErrorCode::CantReclaimRent,
		"user stats too young to reclaim rent. age ={} minimum = {}",
		estimated_user_stats_age,
		THIRTEEN_DAY
	)?;

	Ok(())
}
