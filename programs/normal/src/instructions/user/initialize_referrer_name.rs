use anchor_lang::prelude::*;
use anchor_lang::Discriminator;

use crate::errors::ErrorCode;
use crate::instructions::constraints::*;

use crate::load;
use crate::state::referral::ReferrerName;
use crate::state::user::User;
use crate::state::user_stats::UserStats;
use crate::validate;

#[derive(Accounts)]
#[instruction(
    name: [u8; 32],
)]
pub struct InitializeReferrerName<'info> {
	#[account(
		init,
		seeds = [b"referrer_name", name.as_ref()],
		space = ReferrerName::SIZE,
		bump,
		payer = payer
	)]
	pub referrer_name: AccountLoader<'info, ReferrerName>,
	#[account(
        mut,
        constraint = can_sign_for_user(&user, &authority)?
    )]
	pub user: AccountLoader<'info, User>,
	#[account(
        mut,
        constraint = is_stats_for_user(&user, &user_stats)?
    )]
	pub user_stats: AccountLoader<'info, UserStats>,
	pub authority: Signer<'info>,
	#[account(mut)]
	pub payer: Signer<'info>,
	pub rent: Sysvar<'info, Rent>,
	pub system_program: Program<'info, System>,
}

pub fn handle_initialize_referrer_name(
	ctx: Context<InitializeReferrerName>,
	name: [u8; 32]
) -> Result<()> {
	let authority_key = ctx.accounts.authority.key();
	let user_stats_key = ctx.accounts.user_stats.key();
	let user_key = ctx.accounts.user.key();
	let mut referrer_name = ctx.accounts.referrer_name
		.load_init()
		.or(Err(ErrorCode::UnableToLoadAccountLoader))?;

	let user = load!(ctx.accounts.user)?;
	validate!(
		user.sub_account_id == 0,
		ErrorCode::InvalidReferrer,
		"must be subaccount 0"
	)?;

	referrer_name.authority = authority_key;
	referrer_name.user = user_key;
	referrer_name.user_stats = user_stats_key;
	referrer_name.name = name;

	Ok(())
}
