use anchor_lang::prelude::*;

use crate::load_mut;

use super::AdminUpdateSynthMarket;

#[access_control(synth_market_valid(&ctx.accounts.synth_market))]
pub fn handle_update_synth_market_expiry(
	ctx: Context<AdminUpdateSynthMarket>,
	expiry_ts: i64
) -> Result<()> {
	let clock: Clock = Clock::get()?;
	let synth_market = &mut load_mut!(ctx.accounts.synth_market)?;
	msg!("updating synth market {} expiry", synth_market.market_index);

	validate!(
		clock.unix_timestamp < expiry_ts,
		ErrorCode::DefaultError,
		"Market expiry ts must later than current clock timestamp"
	)?;

	msg!(
		"synth_market.status {:?} -> {:?}",
		synth_market.status,
		update_synth_market_status::ReduceOnly
	);
	msg!("synth_market.expiry_ts {} -> {}", synth_market.expiry_ts, expiry_ts);

	// automatically enter reduce only
	synth_market.status = MarketStatus::ReduceOnly;
	synth_market.expiry_ts = expiry_ts;

	Ok(())
}
