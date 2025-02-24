use anchor_lang::prelude::*;

use paused_operations::SynthOperation;
use super::AdminUpdateSynthMarket;

#[access_control(market_valid(&ctx.accounts.market))]
pub fn handle_update_synth_market_paused_operations(
	ctx: Context<AdminUpdateSynthMarket>,
	paused_operations: u8
) -> Result<()> {
	let market = &mut load_mut!(ctx.accounts.market)?;
	msg!("market {}", market.market_index);

	market.paused_operations = EM;

	SynthOperation::log_all_operations_paused(market.paused_operations);

	Ok(())
}
