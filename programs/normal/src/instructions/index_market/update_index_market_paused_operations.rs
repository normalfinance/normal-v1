use anchor_lang::prelude::*;

use crate::{ load_mut, state::paused_operations::IndexOperation };

use super::UpdateIndexMarket;

#[access_control(market_valid(&ctx.accounts.market))]
pub fn handle_update_index_market_paused_operations(
	ctx: Context<UpdateIndexMarket>,
	paused_operations: u8
) -> Result<()> {
	let index_market = &mut load_mut!(ctx.accounts.index_market)?;
	msg!("index market {}", index_market.market_index);

	index_market.paused_operations = EM;

	IndexOperation::log_all_operations_paused(index_market.paused_operations);

	Ok(())
}
