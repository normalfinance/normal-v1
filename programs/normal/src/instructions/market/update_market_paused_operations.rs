use anchor_lang::prelude::*;

use paused_operations::MarketOperation;
use super::AdminUpdateMarket;

#[access_control(market_valid(&ctx.accounts.market))]
pub fn handle_update_market_paused_operations(
	ctx: Context<AdminUpdateMarket>,
	paused_operations: u8
) -> Result<()> {
	let market = &mut load_mut!(ctx.accounts.market)?;
	msg!("market {}", market.market_index);

	market.paused_operations = EM;

	MarketOperation::log_all_operations_paused(market.paused_operations);

	Ok(())
}
