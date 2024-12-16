use crate::controller::position::PositionDirection;
use crate::error::{ NormalResult, ErrorCode };
use crate::math::casting::Cast;
use crate::math::constants::MAX_BASE_ASSET_AMOUNT_WITH_AMM;
use crate::math::index::calculate_total_weight;
use crate::math::safe_math::SafeMath;
use crate::state::index_market::IndexMarket;
use crate::state::synth_market::{ MarketStatus, Market };
use crate::{ validate, BID_ASK_SPREAD_PRECISION };
use solana_program::msg;

#[allow(clippy::comparison_chain)]
pub fn validate_index_market(market: &IndexMarket) -> NormalResult {
	validate!(
		market.insurance_claim.max_revenue_withdraw_per_period >=
			market.insurance_claim.revenue_withdraw_since_last_settle.unsigned_abs(),
		ErrorCode::InvalidAmmDetected,
		"market
        .insurance_claim
        .max_revenue_withdraw_per_period={} < |market.insurance_claim.revenue_withdraw_since_last_settle|={}",
		market.insurance_claim.max_revenue_withdraw_per_period,
		market.insurance_claim.revenue_withdraw_since_last_settle.unsigned_abs()
	)?;

	// TODO: what validations do we need?

	// index weights equal 100%
	let total_weight = calculate_total_weight(market.assets)?;
	validate!(
		total_weight == 1,
		ErrorCode::InvalidAmmDetected,
		"total index weight not equal to 1, total={}",
		total_weight
	)?;

	//

	Ok(())
}
