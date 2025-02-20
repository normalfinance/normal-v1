use anchor_lang::prelude::*;
use solana_program::program::invoke;
use solana_program::system_instruction::transfer;

use crate::errors::ErrorCode;
use crate::instructions::optional_accounts::get_referrer_and_referrer_stats;
use crate::load;
use crate::load_mut;
use crate::math::safe_math::SafeMath;
use crate::safe_increment;
use crate::state::events::NewUserRecord;
use crate::state::traits::Size;
use crate::state::user::User;
use crate::state::user_stats::UserStats;
use crate::validate;
use crate::State;
use crate::math_error;

#[derive(Accounts)]
#[instruction(
    sub_account_id: u16,
)]
pub struct InitializeUser<'info> {
	#[account(
		init,
		seeds = [
			b"user",
			authority.key.as_ref(),
			sub_account_id.to_le_bytes().as_ref(),
		],
		space = User::SIZE,
		bump,
		payer = payer
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
	#[account(mut)]
	pub payer: Signer<'info>,
	pub rent: Sysvar<'info, Rent>,
	pub system_program: Program<'info, System>,
}

pub fn handle_initialize_user<'c: 'info, 'info>(
	ctx: Context<'_, '_, 'c, 'info, InitializeUser<'info>>,
	sub_account_id: u16,
	name: [u8; 32]
) -> Result<()> {
	let user_key = ctx.accounts.user.key();
	let mut user = ctx.accounts.user
		.load_init()
		.or(Err(ErrorCode::UnableToLoadAccountLoader))?;
	user.authority = ctx.accounts.authority.key();
	user.sub_account_id = sub_account_id;
	user.name = name;

	let remaining_accounts_iter = &mut ctx.remaining_accounts.iter().peekable();

	let mut user_stats = load_mut!(ctx.accounts.user_stats)?;
	user_stats.number_of_sub_accounts =
		user_stats.number_of_sub_accounts.safe_add(1)?;

	// Only try to add referrer if it is the first user
	if user_stats.number_of_sub_accounts_created == 0 {
		let (referrer, referrer_stats) = get_referrer_and_referrer_stats(
			remaining_accounts_iter
		)?;
		let referrer = if
			let (Some(referrer), Some(referrer_stats)) = (referrer, referrer_stats)
		{
			let referrer = load!(referrer)?;
			let mut referrer_stats = load_mut!(referrer_stats)?;

			validate!(referrer.sub_account_id == 0, ErrorCode::InvalidReferrer)?;

			validate!(
				referrer.authority == referrer_stats.authority,
				ErrorCode::ReferrerAndReferrerStatsAuthorityUnequal
			)?;

			referrer_stats.is_referrer = true;

			referrer.authority
		} else {
			Pubkey::default()
		};

		user_stats.referrer = referrer;
	}

	validate!(
		sub_account_id == user_stats.number_of_sub_accounts_created,
		ErrorCode::InvalidUserSubAccountId,
		"Invalid sub account id {}, must be {}",
		sub_account_id,
		user_stats.number_of_sub_accounts_created
	)?;

	user_stats.number_of_sub_accounts_created =
		user_stats.number_of_sub_accounts_created.safe_add(1)?;

	let state = &mut ctx.accounts.state;
	safe_increment!(state.number_of_sub_accounts, 1);

	let max_number_of_sub_accounts = state.max_number_of_sub_accounts();

	validate!(
		max_number_of_sub_accounts == 0 ||
			state.number_of_sub_accounts <= max_number_of_sub_accounts,
		ErrorCode::MaxNumberOfUsers
	)?;

	let now_ts = Clock::get()?.unix_timestamp;

	emit!(NewUserRecord {
		ts: now_ts,
		user_authority: ctx.accounts.authority.key(),
		user: user_key,
		sub_account_id,
		name,
		referrer: user_stats.referrer,
	});

	drop(user);

	let init_fee = state.get_init_user_fee()?;

	if init_fee > 0 {
		let payer_lamports = ctx.accounts.payer.to_account_info().try_lamports()?;
		if payer_lamports < init_fee {
			msg!("payer lamports {} init fee {}", payer_lamports, init_fee);
			return Err(ErrorCode::CantPayUserInitFee.into());
		}

		invoke(
			&transfer(&ctx.accounts.payer.key(), &ctx.accounts.user.key(), init_fee),
			&[
				ctx.accounts.payer.to_account_info(),
				ctx.accounts.user.to_account_info(),
				ctx.accounts.system_program.to_account_info(),
			]
		)?;
	}

	Ok(())
}

#[derive(Accounts)]
#[instruction(
    sub_account_id: u16,
)]
pub struct UpdateUser<'info> {
	#[account(
        mut,
        seeds = [b"user", authority.key.as_ref(), sub_account_id.to_le_bytes().as_ref()],
        bump,
    )]
	pub user: AccountLoader<'info, User>,
	pub authority: Signer<'info>,
}
