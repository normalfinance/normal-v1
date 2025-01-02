use anchor_lang::prelude::*;

use super::AdminUpdateSynthMarket;

pub fn handle_update_synth_market_debt_floor(
	ctx: Context<AdminUpdateSynthMarket>,
	debt_floor: u64
) -> Result<()> {
	ctx.accounts.market.update_debt_floor(debt_floor);

	Ok(())
}
