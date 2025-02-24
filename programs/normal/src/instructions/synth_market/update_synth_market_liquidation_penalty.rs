use anchor_lang::prelude::*;

use super::AdminUpdateSynthMarket;

pub fn handle_update_synth_market_liquidation_penalty(
	ctx: Context<AdminUpdateSynthMarket>,
	liquidation_penalty: u64
) -> Result<()> {
	let market = &mut load_mut!(ctx.accounts.market)?;

	msg!("updating market {} liquidation penalty", market.market_index);

	// validate_margin(
	// 	margin_ratio_initial,
	// 	margin_ratio_maintenance,
	// 	market.liquidator_fee,
	// )?;

	msg!(
		"market.liquidation_penalty: {:?} -> {:?}",
		market.liquidation_penalty,
		liquidation_penalty
	);

	market.liquidation_penalty = liquidation_penalty;
	Ok(())
}
