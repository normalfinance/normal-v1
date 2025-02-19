use anchor_lang::prelude::*;

use crate::load_mut;

use super::AdminUpdateMarket;

#[access_control(market_valid(&ctx.accounts.market))]
pub fn handle_update_market_expiry(
	ctx: Context<AdminUpdateMarket>,
	expiry_ts: i64
) -> Result<()> {
	let clock: Clock = Clock::get()?;
	let market = &mut load_mut!(ctx.accounts.market)?;
	msg!("updating synth market {} expiry", market.market_index);

	validate!(
		clock.unix_timestamp < expiry_ts,
		ErrorCode::DefaultError,
		"Market expiry ts must later than current clock timestamp"
	)?;

	msg!(
		"market.status {:?} -> {:?}",
		market.status,
		update_market_status::ReduceOnly
	);
	msg!("market.expiry_ts {} -> {}", market.expiry_ts, expiry_ts);

	// automatically enter reduce only
	market.status = MarketStatus::ReduceOnly;
	market.expiry_ts = expiry_ts;

	Ok(())
}
