use anchor_lang::prelude::*;

use crate::{
	errors::ErrorCode,
	instructions::initialize_market::AdminUpdateMarket,
	math::amm::MAX_PROTOCOL_FEE_RATE,
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
