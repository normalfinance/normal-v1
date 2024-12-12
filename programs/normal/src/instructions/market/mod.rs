use crate::{ state::market::Market, State };

pub mod initialize_market;
pub mod update_market_liquidation_penalty;
pub mod update_market_debt_ceiling;
pub mod update_market_debt_floor;
pub mod update_market_margin_ratio;
pub mod update_market_synthetic_tier;
pub mod update_market_paused_operations;
pub mod update_market_status;
pub mod update_market_name;
pub mod update_market_liquidation_fee;
pub mod initialize_market_shutdown;
pub mod delete_initialized_market;
pub mod update_market_imf_factor;
pub mod update_market_oracle;
pub mod freeze_market_oracle;
pub mod update_market_number_of_users;
pub mod amm;

#[derive(Accounts)]
pub struct AdminUpdateMarket<'info> {
	pub admin: Signer<'info>,
	#[account(has_one = admin)]
	pub state: Box<Account<'info, State>>,
	#[account(mut)]
	pub market: AccountLoader<'info, Market>,
}
