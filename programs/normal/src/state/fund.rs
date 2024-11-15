use anchor_lang::prelude::*;
use std::cell::Ref;
use std::collections::BTreeMap;

use crate::error::{ NormalResult, ErrorCode };
use crate::math::casting::Cast;
use crate::constants::constants::{
	PRICE_PRECISION,
	PRICE_PRECISION_I64,
	PRICE_PRECISION_U64,
};
use crate::math::safe_math::SafeMath;
use crate::state::oracle::get_oracle_price;
use switchboard::{ AggregatorAccountData, SwitchboardDecimal };
use switchboard_on_demand::{ PullFeedAccountData, SB_ON_DEMAND_PRECISION };

use crate::error::ErrorCode::{ InvalidOracle, UnableToLoadOracle };
use crate::math::safe_unwrap::SafeUnwrap;
use crate::state::load_ref::load_ref;
use crate::state::market::Market;
use crate::state::traits::Size;
use crate::validate;

// #[cfg(test)]
// mod tests;

#[derive(
	Clone,
	Copy,
	BorshSerialize,
	BorshDeserialize,
	PartialEq,
	Debug,
	Eq,
	Default
)]
pub enum WeightingMethod {
	///
	#[default]
	Equal,
	///
	Custom,
	///
	MarketCap,
	///
	SquareRootMarketCap,
}

#[derive(Default, PartialEq, Debug)]
pub struct FundAsset {
	/// The index of the market for this asset
	pub market_index: u16,
	/// The asset's allocation in basis points (i.e 10% = 1000)
	pub weight: u16,
}

pub(crate) type FundAssets = BTreeMap<Pubkey, FundAsset>;

#[derive(Default, Clone, Copy, Debug)]
pub struct Fund {
	/// The address of the market. It is a pda of the market index
	pub pubkey: Pubkey,

	pub admin: Pubkey,

	/// The encoded display name for the index fund e.g. Top 10 Index
	pub name: [u8; 32],

	/// Oracle
	///
	/// The oracle used to price the markets deposits/borrows
	pub oracle: Pubkey,
	pub oracle_source: OracleSource,

	pub weighting_method: WeightingMethod,

	pub assets: FundAssets,

	/// The visibility of the index fund; public = immutable, global; private = mutable, local
	pub public: bool,

	/// Fees
	///
	/// Total taker fee paid (in BPS)
	/// precision: QUOTE_PRECISION
	pub manager_fee: u64,
	/// Total manager fee paid
	/// precision: QUOTE_PRECISION
	pub total_manager_fees: u64,

	/// Timestamps

	pub min_rebalance_ts: i64,
	pub rebalanced_ts: i64,
	pub updated_ts: i64,

	pub padding: [u8; 41],
}

impl Fund {
	pub fn can_rebalance(&self) -> bool {
		self.rebalanced_ts > self.min_rebalance_ts
	}

	pub fn time_since_last_rebalance(&self) -> bool {
		let clock = Clock::get()?;
		let now = clock.unix_timestamp;

		self.rebalanced_ts.safe_sub(now)
	}

	pub fn total_assets(&self) -> u8 {
		self.assets.len()
	}

	pub fn get_total_weight(&self) -> u8 {
		self.assets
			.values()
			.map(|asset| asset.weight)
			.sum::<u8>()
	}

	pub fn update_visibility(&mut self, is_public: bool) -> bool {
		// TODO:
		let third_party_investors = true;

		if self.public && third_party_investors {
			msg!("Publc index funds cannot be updated");
			return Ok(false);
		}
		self.public = is_public;
	}

	pub fn update_asset_weight(
		&mut self,
		asset: Pubkey,
		new_weight: u8
	) -> NormalResult<> {
		if self.public {
			msg!("Publc index funds cannot be updated");
			return Ok(());
		}

		if let Some(asset) = self.assets.get_mut(asset) {
			asset.weight = new_weight;
		} else {
			msg!("Failed to update asset weight");
		}

		return Ok(());
	}
}

pub fn get_index_fund_price(
	price_oracle: &AccountInfo,
	clock_slot: u64,
	multiple: u128,
	is_pull_oracle: bool
) -> NormalResult<OraclePriceData> {
	let oracle_price_data = get_oracle_price(
		&oracle_source,
		&ctx.accounts.oracle,
		Clock::get()?.unix_timestamp.cast()?
	);

	let mut pyth_price_data: &[u8] = &price_oracle
		.try_borrow_data()
		.or(Err(crate::error::ErrorCode::UnableToLoadOracle))?;

	let oracle_price: i64;
	let oracle_conf: u64;
	let mut has_sufficient_number_of_data_points: bool = true;
	let mut oracle_precision: u128;
	let published_slot: u64;

	let price_message = pyth_solana_receiver_sdk::price_update::PriceUpdateV2
		::try_deserialize(&mut pyth_price_data)
		.unwrap();
	oracle_price = price_message.price_message.price;
	oracle_conf = price_message.price_message.conf;
	oracle_precision = (10_u128).pow(
		price_message.price_message.exponent.unsigned_abs()
	);
	published_slot = price_message.posted_slot;

	if oracle_precision <= multiple {
		msg!("Multiple larger than oracle precision");
		return Err(crate::error::ErrorCode::InvalidOracle);
	}
	oracle_precision = oracle_precision.safe_div(multiple)?;

	let mut oracle_scale_mult = 1;
	let mut oracle_scale_div = 1;

	if oracle_precision > PRICE_PRECISION {
		oracle_scale_div = oracle_precision.safe_div(PRICE_PRECISION)?;
	} else {
		oracle_scale_mult = PRICE_PRECISION.safe_div(oracle_precision)?;
	}

	// TODO: calculate nav

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
