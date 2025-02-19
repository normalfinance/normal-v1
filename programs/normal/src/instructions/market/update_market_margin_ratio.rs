use anchor_lang::prelude::*;

use super::AdminUpdateMarket;

#[access_control(market_valid(&ctx.accounts.market))]
pub fn handle_update_market_margin_ratio(
	ctx: Context<AdminUpdateMarket>,
	margin_ratio_initial: u32,
	margin_ratio_maintenance: u32
) -> Result<()> {
	let market = &mut load_mut!(ctx.accounts.market)?;

	msg!("updating market {} margin ratio", market.market_index);

	validate_margin(
		margin_ratio_initial,
		margin_ratio_maintenance,
		market.liquidator_fee
	)?;

	msg!(
		"market.margin_ratio_initial: {:?} -> {:?}",
		market.margin_ratio_initial,
		margin_ratio_initial
	);

	msg!(
		"market.margin_ratio_maintenance: {:?} -> {:?}",
		market.margin_ratio_maintenance,
		margin_ratio_maintenance
	);

	market.margin_ratio_initial = margin_ratio_initial;
	market.margin_ratio_maintenance = margin_ratio_maintenance;
	Ok(())
}
