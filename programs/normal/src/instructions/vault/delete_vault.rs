use anchor_lang::prelude::*;
use anchor_spl::token::{ self, Mint, Token, TokenAccount };

use vault::Vault;
use crate::errors::ErrorCode;
use crate::{ state::*, State };

#[derive(Accounts)]
pub struct DeleteVault<'info> {
	#[account(mut, 
        has_one = authority,
        close = authority
    )]
	pub vault: AccountLoader<'info, Vault>,
	#[account(mut)]
	pub state: Box<Account<'info, State>>,
	pub authority: Signer<'info>,
}

pub fn handle_delete_vault(ctx: Context<DeleteVault>) -> Result<()> {
	let vault = &mut ctx.accounts.vault.load()?;
	msg!("vault {}", vault.vault_index);
	let state = &mut ctx.accounts.state;

	validate!(
		perp_market.number_of_users == 0,
		ErrorCode::InvalidMarketAccountforDeletion,
		"perp_market.number_of_users={} != 0",
		perp_market.number_of_users
	)?;
	validate!(
		vault.market_index == market_index,
		ErrorCode::InvalidMarketAccountforDeletion,
		"market_index={} != vault.market_index={}",
		market_index,
		vault.market_index
	)?;

	// ---

	validate_user_deletion(
		user,
		user_stats,
		&ctx.accounts.state,
		Clock::get()?.unix_timestamp
	)?;

	safe_decrement!(state.number_of_vaults, 1);

	// validate!(
	//     state.number_of_markets - 1 == market_index,
	//     ErrorCode::InvalidMarketAccountforDeletion,
	//     "state.number_of_markets={} != market_index={}",
	//     state.number_of_markets,
	//     market_index
	// )?;
	// validate!(
	//     perp_market.status == MarketStatus::Initialized,
	//     ErrorCode::InvalidMarketAccountforDeletion,
	//     "perp_market.status != Initialized",
	// )?;

	Ok(())
}
