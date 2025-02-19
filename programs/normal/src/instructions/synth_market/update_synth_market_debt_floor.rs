use anchor_lang::prelude::*;

use super::AdminUpdateMarket;

pub fn handle_update_market_debt_floor(
	ctx: Context<AdminUpdateMarket>,
	debt_floor: u64
) -> Result<()> {
	ctx.accounts.market.update_debt_floor(debt_floor);

	Ok(())
}
