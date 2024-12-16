use anchor_lang::prelude::*;

use crate::{ load_mut, state::synth_market::MarketStatus };

use super::UpdateIndexMarket;

#[access_control(index_market_valid(&ctx.accounts.index_market))]
pub fn handle_update_index_market_expiry(
	ctx: Context<UpdateIndexMarket>,
	expiry_ts: i64
) -> Result<()> {
	let clock: Clock = Clock::get()?;
	let index_market = &mut load_mut!(ctx.accounts.index_market)?;
	msg!("updating index market {} expiry", index_market.market_index);

	validate!(
		clock.unix_timestamp < expiry_ts,
		ErrorCode::DefaultError,
		"Market expiry ts must later than current clock timestamp"
	)?;

	msg!(
		"index_market.status {:?} -> {:?}",
		index_market.status,
		MarketStatus::ReduceOnly
	);
	msg!("index_market.expiry_ts {} -> {}", index_market.expiry_ts, expiry_ts);

	// automatically enter reduce only
	index_market.status = MarketStatus::ReduceOnly;
	index_market.expiry_ts = expiry_ts;

	Ok(())
}
