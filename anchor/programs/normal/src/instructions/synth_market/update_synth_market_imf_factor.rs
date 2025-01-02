use anchor_lang::prelude::*;

use crate::error::ErrorCode;
use super::AdminUpdateSynthMarket;

#[access_control(market_valid(&ctx.accounts.market))]
pub fn handle_update_synth_market_imf_factor(
	ctx: Context<AdminUpdateSynthMarket>,
	imf_factor: u32
) -> Result<()> {
	validate!(
		imf_factor <= SPOT_IMF_PRECISION,
		ErrorCode::DefaultError,
		"invalid imf factor"
	)?;
	let market = &mut load_mut!(ctx.accounts.market)?;
	msg!("market {}", market.market_index);

	msg!("market.imf_factor: {:?} -> {:?}", market.imf_factor, imf_factor);

	market.imf_factor = imf_factor;
	Ok(())
}
