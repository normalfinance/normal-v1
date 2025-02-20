use anchor_lang::prelude::*;

use crate::{
	errors::ErrorCode, instructions::update_market_amm::AdminUpdateMarketAMM, math::amm::MAX_FEE_RATE, state::market::Market
};

pub fn handle_set_amm_fee_rate(
	ctx: Context<AdminUpdateMarketAMM>,
	fee_rate: u16
) -> Result<()> {
	let market = &mut ctx.accounts.market.load_init()?;

	if fee_rate > MAX_FEE_RATE {
		return Err(ErrorCode::FeeRateMaxExceeded.into());
	}
	market.amm.fee_rate = fee_rate;

	Ok(())
}
