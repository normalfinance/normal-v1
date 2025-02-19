use crate::controller::position::OrderSide;
use crate::error::{ NormalResult, ErrorCode };
use crate::math::casting::Cast;
use crate::math::safe_math::SafeMath;
use crate::state::oracle::OraclePriceData;
use crate::state::oracles::index_fund::{ IndexFundAssets, WeightingMethod };
use solana_program::msg;

use crate::state::fill_mode::FillMode;
use crate::state::market::Market;
use std::cmp::min;
use std::collections::BTreeMap;

pub fn calculate_total_weight(index_assets: IndexAssets) -> NormalResult<u64> {
	// assumes that missing times are zeros (e.g. handle NaN as 0)
	let prev_twap_99 = data1
		.cast::<u128>()?
		.safe_mul(max(0, weight1_denom.safe_sub(weight1_numer)?).cast::<u128>()?)?
		.safe_div(weight1_denom.cast::<u128>()?)?;

	prev_twap_99.cast::<u64>()?.safe_add(data2)
}

pub fn generate_weights(
	method: WeightingMethod
) -> NormalResult<IndexFundAssets> {
	match method {
		WeightingMethod::Equal => 10000 / (num_assets as u16), // TODO: replace with TEN_THOUSAND constant
		WeightingMethod::Custom => {}
		WeightingMethod::MarketCap => {
			let total_market_cap: f64 = market_caps.iter().sum();
			market_caps
				.iter()
				.map(|cap| (cap / total_market_cap) * 100.0)
				.collect()
		}
		WeightingMethod::SquareRootMarketCap => {
			let sqrt_market_caps: Vec<f64> = market_caps
				.iter()
				.map(|cap| cap.sqrt())
				.collect();
			let total_sqrt_market_cap: f64 = sqrt_market_caps.iter().sum();
			sqrt_market_caps
				.iter()
				.map(|sqrt_cap| (sqrt_cap / total_sqrt_market_cap) * 100.0)
				.collect()
		}
	}
}
