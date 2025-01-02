use anchor_lang::prelude::*;
use index_market::IndexMarket;
use synth_market::{ Market, MarketStatus };

use crate::state::*;

#[derive(Accounts)]
pub struct DeleteInitializedIndexMarket<'info> {
	#[account(mut)]
	pub admin: Signer<'info>,
	#[account(
        mut,
        has_one = admin
    )]
	pub state: Box<Account<'info, State>>,
	#[account(mut, close = admin)]
	pub index_market: AccountLoader<'info, IndexMarket>,
}

pub fn handle_delete_initialized_index_market(
	ctx: Context<DeleteInitializedIndexMarket>,
	market_index: u16
) -> Result<()> {
	let index_market = &mut ctx.accounts.index_market.load()?;
	msg!("index market {}", index_market.market_index);
	let state = &mut ctx.accounts.state;

	// to preserve all protocol invariants, can only remove the last market if it hasn't been "activated"

	validate!(
		state.number_of_index_markets - 1 == market_index,
		ErrorCode::InvalidMarketAccountforDeletion,
		"state.number_of_index_markets={} != market_index={}",
		state.number_of_index_markets,
		market_index
	)?;
	validate!(
		index_market.status == MarketStatus::Initialized,
		ErrorCode::InvalidMarketAccountforDeletion,
		"index_market.status != Initialized"
	)?;
	validate!(
		index_market.number_of_users == 0,
		ErrorCode::InvalidMarketAccountforDeletion,
		"index_market.number_of_users={} != 0",
		index_market.number_of_users
	)?;
	validate!(
		index_market.market_index == market_index,
		ErrorCode::InvalidMarketAccountforDeletion,
		"market_index={} != index_market.market_index={}",
		market_index,
		index_market.market_index
	)?;

	safe_decrement!(state.number_of_index_markets, 1);

	Ok(())
}
