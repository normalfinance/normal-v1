use anchor_lang::prelude::*;

use crate::errors::ErrorCode;
use super::AdminUpdateMarket;

#[access_control(market_valid(&ctx.accounts.market))]
pub fn handle_update_market_liquidation_fee(
	ctx: Context<AdminUpdateMarket>,
	liquidator_fee: u32,
	if_liquidation_fee: u32
) -> Result<()> {
	let market = &mut load_mut!(ctx.accounts.market)?;

	msg!("updating market {} liquidation fee", market.market_index);

	validate!(
		liquidator_fee.safe_add(if_liquidation_fee)? < LIQUIDATION_FEE_PRECISION,
		ErrorCode::DefaultError,
		"Total liquidation fee must be less than 100%"
	)?;

	validate!(
		if_liquidation_fee < LIQUIDATION_FEE_PRECISION,
		ErrorCode::DefaultError,
		"If liquidation fee must be less than 100%"
	)?;

	validate_margin(
		market.margin_ratio_initial,
		market.margin_ratio_maintenance,
		liquidator_fee
	)?;

	msg!(
		"market.liquidator_fee: {:?} -> {:?}",
		market.liquidator_fee,
		liquidator_fee
	);

	msg!(
		"market.if_liquidation_fee: {:?} -> {:?}",
		market.if_liquidation_fee,
		if_liquidation_fee
	);

	market.liquidator_fee = liquidator_fee;
	market.if_liquidation_fee = if_liquidation_fee;
	Ok(())
}
