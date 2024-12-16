use anchor_lang::prelude::*;
use anchor_spl::token::{ self, Token, TokenAccount };

use crate::{
	controller,
	errors::ErrorCode,
	load_mut,
	state::synth_market::SynthMarket,
	util::transfer_from_owner_to_vault,
};

#[derive(Accounts)]
#[instruction(market_index: u16,)]
pub struct MintIndexTokens<'info> {
	#[account(mut)]
	pub user: AccountLoader<'info, User>,
	#[account(
        mut,
        constraint = is_stats_for_user(&user, &user_stats)?
    )]
	pub user_stats: AccountLoader<'info, UserStats>,
	#[account(
        mut,
        seeds = [b"index_market_vault".as_ref(), market_index.to_le_bytes().as_ref()],
        bump,
    )]
	pub index_market_vault: Box<InterfaceAccount<'info, TokenAccount>>,
	pub token_program: Interface<'info, TokenInterface>,
}

pub fn handle_index_mint(
	ctx: Context<MintIndexTokens>,
	amount: u64
) -> Result<()> {
	let clock = Clock::get()?;
	let now = clock.unix_timestamp;

	let index_market = &mut load_mut!(ctx.accounts.index_market)?;

	let user_key = ctx.accounts.user.key();

	let user = &mut load_mut!(ctx.accounts.user)?;

	validate!(!user.is_being_liquidated(), ErrorCode::LiquidationsOngoing)?;
	validate!(!user.is_bankrupt(), ErrorCode::UserBankrupt, "user bankrupt")?;

	let previouse_index_token_balance = ctx.accounts.token_owner_account.amount;

	// Transfer <amount> of SOL into market
	transfer_from_owner_to_vault(
		&ctx.accounts.authority,
		&ctx.accounts.token_owner_account,
		&ctx.accounts.index_market_vault,
		&ctx.accounts.token_program,
		amount
	)?;

	// Calculate swap amounts
	let swap_amounts = index_market.compute_swap_amounts(amount);

	// Loop through <swap_amounts> and execute each swap
	{
		// ...
	}

	// Update user stats
	let mut user_stats = load_mut!(ctx.accounts.user_stats)?;

	// Update the index market
	// ...
	index_market.total_minted = index_market.total_minted.safe_add(amount);

	if previouse_index_token_balance == 0 {
		index_market.number_of_users = index_market.number_of_users.safe_add(1);
	}

	emit!(IndexMintRecord {
		market_index: index_market.market_index,
		user: user_key,
		oracle_price: 0, // TODO:
		quote_asset_amount: amount,
		ts: now,
	});

	Ok(())
}
