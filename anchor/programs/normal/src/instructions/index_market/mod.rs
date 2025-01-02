use crate::{ state::index_market::IndexMarket, State };

pub mod initialize_index_market;
pub mod rebalance_index_market;
pub mod update_index_market_visibility;
pub mod update_index_market_whitelist;
pub mod collect_index_market_protocol_fees;
pub mod update_index_market_expiry;
pub mod update_index_market_expense_ratio;
pub mod update_index_market_revenue_share;
pub mod update_index_market_paused_operations;
pub mod delete_initialized_index_market;
pub mod token;

#[derive(Accounts)]
pub struct UpdateIndexMarket<'info> {
	pub admin: Signer<'info>,
	#[account(has_one = admin)]
	pub state: Box<Account<'info, State>>,
	#[account(mut)]
	pub index_market: AccountLoader<'info, IndexMarket>,
}
