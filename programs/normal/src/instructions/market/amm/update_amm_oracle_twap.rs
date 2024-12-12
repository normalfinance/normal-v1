use anchor_lang::prelude::*;

use crate::instructions::constraints::{ market_valid, valid_oracle_for_amm };
use crate::{ state::{ self, market::Market }, State };

use super::reset_amm_oracle_twap::RepegCurve;

#[access_control(
    market_valid(&ctx.accounts.market)
    valid_oracle_for_amm(&ctx.accounts.oracle, &ctx.accounts.market)
)]
pub fn handle_update_amm_oracle_twap(ctx: Context<RepegCurve>) -> Result<()> {
	// allow update to amm's oracle twap iff price gap is reduced and thus more tame funding
	// otherwise if oracle error or funding flip: set oracle twap to mark twap (0 gap)

	let clock = Clock::get()?;
	let now = clock.unix_timestamp;

	let market = &mut load_mut!(ctx.accounts.market)?;
	msg!("updating amm oracle twap for market {}", market.market_index);
	let price_oracle = &ctx.accounts.oracle;
	let oracle_twap = market.amm.get_oracle_twap(price_oracle, clock.slot)?;

	if let Some(oracle_twap) = oracle_twap {
		let oracle_mark_gap_before = market.amm.last_mark_price_twap
			.cast::<i64>()?
			.safe_sub(market.amm.historical_oracle_data.last_oracle_price_twap)?;

		let oracle_mark_gap_after = market.amm.last_mark_price_twap
			.cast::<i64>()?
			.safe_sub(oracle_twap)?;

		if
			(oracle_mark_gap_after > 0 && oracle_mark_gap_before < 0) ||
			(oracle_mark_gap_after < 0 && oracle_mark_gap_before > 0)
		{
			msg!(
				"market.amm.historical_oracle_data.last_oracle_price_twap {} -> {}",
				market.amm.historical_oracle_data.last_oracle_price_twap,
				market.amm.last_mark_price_twap.cast::<i64>()?
			);
			msg!(
				"market.amm.historical_oracle_data.last_oracle_price_twap_ts {} -> {}",
				market.amm.historical_oracle_data.last_oracle_price_twap_ts,
				now
			);
			market.amm.historical_oracle_data.last_oracle_price_twap =
				market.amm.last_mark_price_twap.cast::<i64>()?;
			market.amm.historical_oracle_data.last_oracle_price_twap_ts = now;
		} else if
			oracle_mark_gap_after.unsigned_abs() <=
			oracle_mark_gap_before.unsigned_abs()
		{
			msg!(
				"market.amm.historical_oracle_data.last_oracle_price_twap {} -> {}",
				market.amm.historical_oracle_data.last_oracle_price_twap,
				oracle_twap
			);
			msg!(
				"market.amm.historical_oracle_data.last_oracle_price_twap_ts {} -> {}",
				market.amm.historical_oracle_data.last_oracle_price_twap_ts,
				now
			);
			market.amm.historical_oracle_data.last_oracle_price_twap = oracle_twap;
			market.amm.historical_oracle_data.last_oracle_price_twap_ts = now;
		} else {
			return Err(ErrorCode::PriceBandsBreached.into());
		}
	} else {
		return Err(ErrorCode::InvalidOracle.into());
	}

	Ok(())
}
