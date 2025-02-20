use anchor_lang::prelude::*;

use crate::{
	errors::ErrorCode,
	state::market::MarketStatus,
	instructions::constraints::market_valid,
};
use super::AdminUpdateMarket;

#[access_control(market_valid(&ctx.accounts.market))]
pub fn handle_update_market_status(
	ctx: Context<AdminUpdateMarket>,
	status: MarketStatus
) -> Result<()> {
	validate!(
		!matches!(status, MarketStatus::Delisted | MarketStatus::Settlement),
		ErrorCode::DefaultError,
		"must set settlement/delist through another instruction"
	)?;

	status.validate_not_deprecated()?;

	let market = &mut load_mut!(ctx.accounts.market)?;

	msg!("market {}", market.market_index);

	msg!("market.status: {:?} -> {:?}", market.status, status);

	market.status = status;
	Ok(())
}
