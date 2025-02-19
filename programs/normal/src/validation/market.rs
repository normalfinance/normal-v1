use crate::controller::position::PositionDirection;
use crate::error::{ NormalResult, ErrorCode };
use crate::math::casting::Cast;
use crate::math::constants::MAX_BASE_ASSET_AMOUNT_WITH_AMM;
use crate::math::safe_math::SafeMath;

use crate::state::market::{ MarketStatus, Market };
use crate::{ validate, BID_ASK_SPREAD_PRECISION };
use solana_program::msg;

#[allow(clippy::comparison_chain)]
pub fn validate_market(market: &Market) -> NormalResult {
	// let (_, remainder_base_asset_amount_long) =
	// 	crate::math::orders::standardize_base_asset_amount_with_remainder_i128(
	// 		amm.base_asset_amount_long,
	// 		amm.order_step_size.cast()?
	// 	)?;

	// let (_, remainder_base_asset_amount_short) =
	// 	crate::math::orders::standardize_base_asset_amount_with_remainder_i128(
	// 		amm.base_asset_amount_short,
	// 		amm.order_step_size.cast()?
	// 	)?;

	// validate!(
	// 	remainder_base_asset_amount_long == 0 &&
	// 		remainder_base_asset_amount_short == 0,
	// 	ErrorCode::InvalidPositionDelta,
	// 	"invalid base_asset_amount_long/short vs order_step_size, remainder={}/{}",
	// 	remainder_base_asset_amount_short,
	// 	amm.order_step_size
	// )?;
	// validate!(
	// 	amm.base_asset_amount_long + amm.base_asset_amount_short ==
	// 		amm.base_asset_amount_with_amm +
	// 			amm.base_asset_amount_with_unsettled_lp,
	// 	ErrorCode::InvalidAmmDetected,
	// 	"Market NET_BAA Error:
	//     amm.base_asset_amount_long={},
	//     + amm.base_asset_amount_short={}
	//     !=
	//     amm.base_asset_amount_with_amm={}
	//     +  amm.base_asset_amount_with_unsettled_lp={}",
	// 	amm.base_asset_amount_long,
	// 	amm.base_asset_amount_short,
	// 	amm.base_asset_amount_with_amm,
	// 	amm.base_asset_amount_with_unsettled_lp
	// )?;

	// validate!(
	// 	amm.base_asset_amount_with_amm <=
	// 		(MAX_BASE_ASSET_AMOUNT_WITH_AMM as i128),
	// 	ErrorCode::InvalidAmmDetected,
	// 	"amm.base_asset_amount_with_amm={} is too large",
	// 	amm.base_asset_amount_with_amm
	// )?;

	// validate!(
	// 	amm.peg_multiplier > 0,
	// 	ErrorCode::InvalidAmmDetected,
	// 	"peg_multiplier out of wack"
	// )?;

	if market.status != MarketStatus::ReduceOnly {
		validate!(
			amm.sqrt_k > amm.base_asset_amount_with_amm.unsigned_abs(),
			ErrorCode::InvalidAmmDetected,
			"k out of wack: k={}, net_baa={}",
			amm.sqrt_k,
			amm.base_asset_amount_with_amm
		)?;
	}

	// validate!(
	// 	amm.sqrt_k >= amm.base_asset_reserve ||
	// 		amm.sqrt_k >= amm.quote_asset_reserve,
	// 	ErrorCode::InvalidAmmDetected,
	// 	"k out of wack: k={}, bar={}, qar={}",
	// 	amm.sqrt_k,
	// 	amm.base_asset_reserve,
	// 	amm.quote_asset_reserve
	// )?;

	// validate!(
	// 	amm.sqrt_k >= amm.user_lp_shares,
	// 	ErrorCode::InvalidAmmDetected,
	// 	"amm.sqrt_k < amm.user_lp_shares: {} < {}",
	// 	amm.sqrt_k,
	// 	amm.user_lp_shares
	// )?;

	// let invariant_sqrt_u192 = crate::bn::U192::from(amm.sqrt_k);
	// let invariant = invariant_sqrt_u192.safe_mul(invariant_sqrt_u192)?;
	// let quote_asset_reserve = invariant
	// 	.safe_div(crate::bn::U192::from(amm.base_asset_reserve))?
	// 	.try_to_u128()?;

	// let rounding_diff = quote_asset_reserve
	// 	.cast::<i128>()?
	// 	.safe_sub(amm.quote_asset_reserve.cast()?)?
	// 	.abs();

	// validate!(
	// 	rounding_diff <= 15,
	// 	ErrorCode::InvalidAmmDetected,
	// 	"qar/bar/k out of wack: k={}, bar={}, qar={}, qar'={} (rounding: {})",
	// 	invariant,
	// 	amm.base_asset_reserve,
	// 	amm.quote_asset_reserve,
	// 	quote_asset_reserve,
	// 	rounding_diff
	// )?;

	// // todo
	// if amm.base_spread > 0 {
	// 	// bid quote/base < reserve q/b
	// 	validate!(
	// 		amm.bid_base_asset_reserve >= amm.base_asset_reserve &&
	// 			amm.bid_quote_asset_reserve <= amm.quote_asset_reserve,
	// 		ErrorCode::InvalidAmmDetected,
	// 		"bid reserves out of wack: {} -> {}, quote: {} -> {}",
	// 		amm.bid_base_asset_reserve,
	// 		amm.base_asset_reserve,
	// 		amm.bid_quote_asset_reserve,
	// 		amm.quote_asset_reserve
	// 	)?;

	// 	// ask quote/base > reserve q/b
	// 	validate!(
	// 		amm.ask_base_asset_reserve <= amm.base_asset_reserve &&
	// 			amm.ask_quote_asset_reserve >= amm.quote_asset_reserve,
	// 		ErrorCode::InvalidAmmDetected,
	// 		"ask reserves out of wack base: {} -> {}, quote: {} -> {}",
	// 		amm.ask_base_asset_reserve,
	// 		amm.base_asset_reserve,
	// 		amm.ask_quote_asset_reserve,
	// 		amm.quote_asset_reserve
	// 	)?;
	// }

	// validate!(
	// 	amm.long_spread + amm.short_spread >= amm.base_spread,
	// 	ErrorCode::InvalidAmmDetected,
	// 	"long_spread + short_spread < base_spread: {} + {} < {}",
	// 	amm.long_spread,
	// 	amm.short_spread,
	// 	amm.base_spread
	// )?;

	// validate!(
	// 	amm.long_spread.safe_add(amm.short_spread)?.cast::<u64>()? <=
	// 		BID_ASK_SPREAD_PRECISION,
	// 	ErrorCode::InvalidAmmDetected,
	// 	"long_spread {} + short_spread {} > max bid-ask spread precision (max spread = {})",
	// 	amm.long_spread,
	// 	amm.short_spread,
	// 	amm.max_spread
	// )?;

	// if amm.base_asset_amount_with_amm > 0 {
	// 	// users are long = removed base and added quote = qar increased
	// 	// bid quote/base < reserve q/b
	// 	validate!(
	// 		amm.terminal_quote_asset_reserve <= amm.quote_asset_reserve,
	// 		ErrorCode::InvalidAmmDetected,
	// 		"terminal_quote_asset_reserve out of wack"
	// 	)?;
	// } else if amm.base_asset_amount_with_amm < 0 {
	// 	validate!(
	// 		amm.terminal_quote_asset_reserve >= amm.quote_asset_reserve,
	// 		ErrorCode::InvalidAmmDetected,
	// 		"terminal_quote_asset_reserve out of wack (terminal <) {} > {}",
	// 		amm.terminal_quote_asset_reserve,
	// 		amm.quote_asset_reserve
	// 	)?;
	// } else {
	// 	validate!(
	// 		amm.terminal_quote_asset_reserve == amm.quote_asset_reserve,
	// 		ErrorCode::InvalidAmmDetected,
	// 		"terminal_quote_asset_reserve out of wack {}!={}",
	// 		amm.terminal_quote_asset_reserve,
	// 		amm.quote_asset_reserve
	// 	)?;
	// }

	// if amm.base_spread > 0 {
	// 	validate!(
	// 		amm.max_spread > amm.base_spread &&
	// 			amm.max_spread < market.margin_ratio_initial * 100,
	// 		ErrorCode::InvalidAmmDetected,
	// 		"invalid max_spread"
	// 	)?;
	// }

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

	// validate!(
	// 	amm.base_asset_amount_per_lp <
	// 		(MAX_BASE_ASSET_AMOUNT_WITH_AMM as i128),
	// 	ErrorCode::InvalidAmmDetected,
	// 	"amm.base_asset_amount_per_lp too large: {}",
	// 	amm.base_asset_amount_per_lp
	// )?;

	// validate!(
	// 	amm.quote_asset_amount_per_lp <
	// 		(MAX_BASE_ASSET_AMOUNT_WITH_AMM as i128),
	// 	ErrorCode::InvalidAmmDetected,
	// 	"amm.quote_asset_amount_per_lp too large: {}",
	// 	amm.quote_asset_amount_per_lp
	// )?;

	Ok(())
}
