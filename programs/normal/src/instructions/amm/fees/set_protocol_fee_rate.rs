use anchor_lang::prelude::*;

use crate::{
	error::ErrorCode,
	instructions::AdminUpdateMarket,
	math::amm::MAX_PROTOCOL_FEE_RATE,
	state::market::Market,
};

pub fn handle_set_protocol_fee_rate(
	ctx: Context<AdminUpdateMarket>,
	protocol_fee_rate: u16
) -> Result<()> {
	if protocol_fee_rate > MAX_PROTOCOL_FEE_RATE {
		return Err(ErrorCode::ProtocolFeeRateMaxExceeded.into());
	}
	self.protocol_fee_rate = protocol_fee_rate;

	Ok(())
}
