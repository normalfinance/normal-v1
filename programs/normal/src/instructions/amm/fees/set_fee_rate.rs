use anchor_lang::prelude::*;

use crate::{
	error::ErrorCode,
	instructions::AdminUpdateSynthMarket,
	math::amm::MAX_FEE_RATE,
	state::synth_market::SynthMarket,
};

pub fn handle_set_amm_fee_rate(
	ctx: Context<AdminUpdateSynthMarket>,
	fee_rate: u16
) -> Result<()> {
	let market = &mut ctx.accounts.market.load_init()?;

	if fee_rate > MAX_FEE_RATE {
		return Err(ErrorCode::FeeRateMaxExceeded.into());
	}
	amm.fee_rate = fee_rate;

	Ok(())
}
