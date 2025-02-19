use anchor_lang::prelude::*;
use market::{ Market, MarketStatus };

use crate::state::*;

use super::update_market_liquidation_penalty::AdminUpdateMarket;

#[derive(Accounts)]
pub struct DeleteInitializedMarket<'info> {
	#[account(mut)]
	pub admin: Signer<'info>,
	#[account(
        mut,
        has_one = admin
    )]
	pub state: Box<Account<'info, State>>,
	#[account(mut, close = admin)]
	pub market: AccountLoader<'info, Market>,
}

pub fn handle_delete_initialized_market(
	ctx: Context<DeleteInitializedMarket>,
	market_index: u16
) -> Result<()> {
	let market = &mut ctx.accounts.market.load()?;
	msg!("market {}", market.market_index);
	let state = &mut ctx.accounts.state;

	// to preserve all protocol invariants, can only remove the last market if it hasn't been "activated"

	validate!(
		state.number_of_markets - 1 == market_index,
		ErrorCode::InvalidMarketAccountforDeletion,
		"state.number_of_markets={} != market_index={}",
		state.number_of_markets,
		market_index
	)?;
	validate!(
		market.status == MarketStatus::Initialized,
		ErrorCode::InvalidMarketAccountforDeletion,
		"market.status != Initialized"
	)?;
	validate!(
		market.market_index == market_index,
		ErrorCode::InvalidMarketAccountforDeletion,
		"market_index={} != market.market_index={}",
		market_index,
		market.market_index
	)?;

	safe_decrement!(state.number_of_markets, 1);

	Ok(())
}
