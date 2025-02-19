use anchor_lang::prelude::*;

use super::AdminUpdateMarket;

#[access_control(market_valid(&ctx.accounts.market))]
pub fn handle_update_market_debt_ceiling(
	ctx: Context<AdminUpdateMarket>,
	debt_ceiling: u64
) -> Result<()> {
	ctx.accounts.market.update_debt_ceiling(debt_ceiling);

	Ok(())
}
