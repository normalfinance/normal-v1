use anchor_lang::prelude::*;

use crate::errors::{ NormalResult, ErrorCode };
use crate::math::casting::Cast;
use crate::constants::main::{
	PRICE_PRECISION,
	PRICE_PRECISION_I64,
	PRICE_PRECISION_U64,
};
use crate::math::safe_math::SafeMath;

use crate::errors::ErrorCode::{ InvalidOracle, UnableToLoadOracle };
use crate::math::safe_unwrap::SafeUnwrap;
use crate::validate;

// #[cfg(test)]
// mod tests;

#[derive(
	Default,
	AnchorSerialize,
	AnchorDeserialize,
	Clone,
	Copy,
	Eq,
	PartialEq,
	Debug
)]
pub struct HistoricalOracleData {
	/// precision: PRICE_PRECISION
	pub last_oracle_price: i64,
	/// precision: PRICE_PRECISION
	pub last_oracle_conf: u64,
	/// number of slots since last update
	pub last_oracle_delay: i64,
	/// precision: PRICE_PRECISION
	pub last_oracle_price_twap_5min: i64,
	/// unix_timestamp of last snapshot
	pub last_oracle_price_twap_ts: i64,
}

impl HistoricalOracleData {
	pub fn default_quote_oracle() -> Self {
		HistoricalOracleData {
			last_oracle_price: PRICE_PRECISION_I64,
			last_oracle_conf: 0,
			last_oracle_delay: 0,
			last_oracle_price_twap_5min: PRICE_PRECISION_I64,
			..HistoricalOracleData::default()
		}
	}

	pub fn default_price(price: i64) -> Self {
		HistoricalOracleData {
			last_oracle_price: price,
			last_oracle_conf: 0,
			last_oracle_delay: 10,
			last_oracle_price_twap_5min: price,
			..HistoricalOracleData::default()
		}
	}

	pub fn default_with_current_oracle(
		oracle_price_data: OraclePriceData
	) -> Self {
		HistoricalOracleData {
			last_oracle_price: oracle_price_data.price,
			last_oracle_conf: oracle_price_data.confidence,
			last_oracle_delay: oracle_price_data.delay,
			last_oracle_price_twap_5min: oracle_price_data.price,
			// last_oracle_price_twap_ts: now,
			..HistoricalOracleData::default()
		}
	}
}

#[derive(
	AnchorSerialize,
	AnchorDeserialize,
	Clone,
	Copy,
	Eq,
	PartialEq,
	Debug,
	Default
)]
pub enum OracleSource {
	#[default]
	Pyth,
	QuoteAsset,
	Pyth1K,
	Pyth1M,
	PythStableCoin,
	PythPull,
	Pyth1KPull,
	Pyth1MPull,
	PythStableCoinPull,
}

#[derive(Default, Clone, Copy, Debug)]
pub struct OraclePriceData {
	pub price: i64,
	pub confidence: u64,
	pub delay: i64,
	pub has_sufficient_number_of_data_points: bool,
}

impl OraclePriceData {
	pub fn default_usd() -> Self {
		OraclePriceData {
			price: PRICE_PRECISION_I64,
			confidence: 1,
			delay: 0,
			has_sufficient_number_of_data_points: true,
		}
	}
}

pub fn get_oracle_price(
	oracle_source: &OracleSource,
	price_oracle: &AccountInfo,
	clock_slot: u64
) -> NormalResult<OraclePriceData> {
	match oracle_source {
		OracleSource::Pyth => get_pyth_price(price_oracle, clock_slot, 1, false),
		OracleSource::Pyth1K =>
			get_pyth_price(price_oracle, clock_slot, 1000, false),
		OracleSource::Pyth1M =>
			get_pyth_price(price_oracle, clock_slot, 1000000, false),
		OracleSource::PythStableCoin =>
			get_pyth_stable_coin_price(price_oracle, clock_slot, false),
	
		OracleSource::QuoteAsset =>
			Ok(OraclePriceData {
				price: PRICE_PRECISION_I64,
				confidence: 1,
				delay: 0,
				has_sufficient_number_of_data_points: true,
			}),
		OracleSource::PythPull => get_pyth_price(price_oracle, clock_slot, 1, true),
		OracleSource::Pyth1KPull =>
			get_pyth_price(price_oracle, clock_slot, 1000, true),
		OracleSource::Pyth1MPull =>
			get_pyth_price(price_oracle, clock_slot, 1000000, true),
		OracleSource::PythStableCoinPull => {
			get_pyth_stable_coin_price(price_oracle, clock_slot, true)
		}
	}
}

pub fn get_pyth_price(
	price_oracle: &AccountInfo,
	clock_slot: u64,
	multiple: u128,
	is_pull_oracle: bool
) -> NormalResult<OraclePriceData> {
	let mut pyth_price_data: &[u8] = &price_oracle
		.try_borrow_data()
		.or(Err(crate::errors::ErrorCode::UnableToLoadOracle))?;

	let oracle_price: i64;
	let oracle_conf: u64;
	let mut has_sufficient_number_of_data_points: bool = true;
	let mut oracle_precision: u128;
	let published_slot: u64;

	if is_pull_oracle {
		let price_message = pyth_solana_receiver_sdk::price_update::PriceUpdateV2
			::try_deserialize(&mut pyth_price_data)
			.unwrap();
		oracle_price = price_message.price_message.price;
		oracle_conf = price_message.price_message.conf;
		oracle_precision = (10_u128).pow(
			price_message.price_message.exponent.unsigned_abs()
		);
		published_slot = price_message.posted_slot;
	} else {
		let price_data = pyth_client::cast::<pyth_client::Price>(pyth_price_data);
		oracle_price = price_data.agg.price;
		oracle_conf = price_data.agg.conf;
		let min_publishers = price_data.num.min(3);
		let publisher_count = price_data.num_qt;

		#[cfg(feature = "mainnet-beta")]
		{
			has_sufficient_number_of_data_points = publisher_count >= min_publishers;
		}
		#[cfg(not(feature = "mainnet-beta"))]
		{
			has_sufficient_number_of_data_points = true;
		}

		oracle_precision = (10_u128).pow(price_data.expo.unsigned_abs());
		published_slot = price_data.valid_slot;
	}

	if oracle_precision <= multiple {
		msg!("Multiple larger than oracle precision");
		return Err(crate::errors::ErrorCode::InvalidOracle);
	}
	oracle_precision = oracle_precision.safe_div(multiple)?;

	let mut oracle_scale_mult = 1;
	let mut oracle_scale_div = 1;

	if oracle_precision > PRICE_PRECISION {
		oracle_scale_div = oracle_precision.safe_div(PRICE_PRECISION)?;
	} else {
		oracle_scale_mult = PRICE_PRECISION.safe_div(oracle_precision)?;
	}

	let oracle_price_scaled = oracle_price
		.cast::<i128>()?
		.safe_mul(oracle_scale_mult.cast()?)?
		.safe_div(oracle_scale_div.cast()?)?
		.cast::<i64>()?;

	let oracle_conf_scaled = oracle_conf
		.cast::<u128>()?
		.safe_mul(oracle_scale_mult)?
		.safe_div(oracle_scale_div)?
		.cast::<u64>()?;

	let oracle_delay: i64 = clock_slot
		.cast::<i64>()?
		.safe_sub(published_slot.cast()?)?;

	Ok(OraclePriceData {
		price: oracle_price_scaled,
		confidence: oracle_conf_scaled,
		delay: oracle_delay,
		has_sufficient_number_of_data_points,
	})
}

pub fn get_pyth_stable_coin_price(
	price_oracle: &AccountInfo,
	clock_slot: u64,
	is_pull_oracle: bool
) -> NormalResult<OraclePriceData> {
	let mut oracle_price_data = get_pyth_price(
		price_oracle,
		clock_slot,
		1,
		is_pull_oracle
	)?;

	let price = oracle_price_data.price;
	let confidence = oracle_price_data.confidence;
	let five_bps = 500_i64;

	if
		price.safe_sub(PRICE_PRECISION_I64)?.abs() <=
		five_bps.min(confidence.cast()?)
	{
		oracle_price_data.price = PRICE_PRECISION_I64;
	}

	Ok(oracle_price_data)
}


#[derive(Clone, Copy)]
pub struct StrictOraclePrice {
	pub current: i64,
	pub twap_5min: Option<i64>,
}

impl StrictOraclePrice {
	pub fn new(price: i64, twap_5min: i64, enabled: bool) -> Self {
		Self {
			current: price,
			twap_5min: if enabled {
				Some(twap_5min)
			} else {
				None
			},
		}
	}

	pub fn max(&self) -> i64 {
		match self.twap_5min {
			Some(twap) => self.current.max(twap),
			None => self.current,
		}
	}

	pub fn min(&self) -> i64 {
		match self.twap_5min {
			Some(twap) => self.current.min(twap),
			None => self.current,
		}
	}

	pub fn validate(&self) -> NormalResult {
		validate!(
			self.current > 0,
			ErrorCode::InvalidOracle,
			"oracle_price_data={} (<= 0)",
			self.current
		)?;

		if let Some(twap) = self.twap_5min {
			validate!(
				twap > 0,
				ErrorCode::InvalidOracle,
				"oracle_price_twap={} (<= 0)",
				twap
			)?;
		}

		Ok(())
	}
}

#[cfg(test)]
impl StrictOraclePrice {
	pub fn test(price: i64) -> Self {
		Self {
			current: price,
			twap_5min: None,
		}
	}
}
