use anchor_lang::prelude::*;
use std::cell::Ref;
use std::collections::BTreeMap;

use crate::error::{ NormalResult, ErrorCode };
use crate::math::casting::Cast;
use crate::constants::main::{
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
pub enum IndexWeighting {
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
pub enum IndexVisibility {
	#[default]
	Private, // mutable
	Public, // immutable
}

#[derive(Default, PartialEq, Debug)]
pub struct IndexAsset {
	pub mint: Pubkey,
	pub vault: Pubkey,
	pub market_index: u16,
	/// The asset's allocation in basis points
	pub weight: u16,
	pub last_updated_ts: i64,
}

pub(crate) type IndexAssets = BTreeMap<Pubkey, IndexAsset>;

#[account]
pub struct IndexMarket {
	/// The index market's address. It is a pda of the market index
	pub pubkey: Pubkey,
	/// The owner/authority of the account
	pub authority: Pubkey,
	/// An addresses that can control the account on the authority's behalf. Has limited power, cant withdraw
	pub delegate: Pubkey,

	// TODO: move these to the AMM
	pub fee_authority: Pubkey,
	pub whitelist_authority: Pubkey,
	pub rebalance_authority: Pubkey,

	pub market_index: u16,
	/// Encoded display name for the market e.g. BTC-SOL
	pub name: [u8; 32],
	/// Whether a market is active, reduce only, expired, etc
	/// Affects whether users can open/close positions
	pub status: MarketStatus,
	pub paused_operations: u8,
	pub number_of_users: u32,

	// Oracle
	//
	pub oracle: Pubkey,
	pub oracle_source: OracleSource,

	/// Index
	///
	pub weighting: IndexWeighting,
	pub assets: IndexAssets,
	/// The visibility of the index fund
	pub visibility: IndexVisibility,
	/// List of accounts allowed to purchase the index
	pub whitelist: Vec<Pubkey>,
	pub blacklist: Vec<Pubkey>,

	/// Fees
	/// 
	/// Total taker fee paid in basis points
	pub expense_ratio: u64,
	///
	pub revenue_share: u64,
	pub protocol_fee_owed: u64,
	pub manager_fee_owed: u64,
	pub referral_fee_owed: u64,
	pub total_fees: u64,

	// AMM
	//
	pub amm: AMM,

	// Insurance
	//
	/// The market's claim on the insurance fund
	pub insurance_claim: InsuranceClaim,

	// Metrics
	//
	

	// Shutdown
	//
	/// The ts when the market will be expired. Only set if market is in reduce only mode
	pub expiry_ts: i64,
	/// The price at which positions will be settled. Only set if market is expired
	/// precision = PRICE_PRECISION
	pub expiry_price: i64,

	// Timestamps
	pub rebalanced_ts: i64,
	pub updated_ts: i64,

	pub padding: [u8; 43],
}

impl Default for IndexMarket {
	fn default() -> Self {
		IndexMarket {
			pubkey: Pubkey::default(),
			authority: Pubkey::default(),
			delegate: Pubkey::default(),
			market_index: 0,
			name: [0; 32],
			status: MarketStatus::default(),
			paused_operations: 0,
			number_of_users: 0,

			oracle: Pubkey::default(),
			oracle_source: OracleSource::default(),

			weighting: IndexWeighting::default(),
			assets: IndexAssets::default(),
			visibility: IndexVisibility::default(),
			whitelist: [],
			manager_fee: 0,
			total_manager_fees: 0,
			min_rebalance_ts: 0,
			rebalanced_ts: 0,
			updated_ts: 0,

			token_mint_collateral: Pubkey::default(),
			token_vault_synthetic: Pubkey::default(),
			token_vault_collateral: Pubkey::default(),

			amm: AMM {
				oracle: 0,
				oracle_source,
				historical_oracle_data: HistoricalOracleData::default(),
				last_oracle_conf_pct: 0,
				last_oracle_valid: false,
				last_oracle_normalised_price: 0,
				last_oracle_reserve_price_spread_pct: 0,
				oracle_std: 0,

				tick_spacing,
				tick_spacing_seed: tick_spacing.to_le_bytes(),

				liquidity: 0,
				sqrt_price,
				tick_current_index: tick_index_from_sqrt_price(&sqrt_price),

				fee_rate: 0,
				protocol_fee_rate: 0,

				protocol_fee_owed_synthetic: 0,
				protocol_fee_owed_quote: 0,

				token_mint_synthetic,
				token_vault_synthetic,
				fee_growth_global_synthetic: 0,

				token_mint_quote,
				token_vault_quote,
				fee_growth_global_quote: 0,

				reward_infos: [],
			},

			insurance_claim: InsuranceClaim::default(),

			outstanding_debt: 0,

			expiry_ts: 0,
			expiry_price: 0,

			padding: [0; 43],
		}
	}
}

impl Size for IndexMarket {
	const SIZE: usize = 1216; // TODO:
}

impl IndexMarket {
	pub fn can_invest(&self, account: Pubkey) -> bool {
		self.whitelist.contains(&account)
	}

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

	pub fn update_visibility(
		&mut self,
		new_visibility: IndexFundVisibility
	) -> bool {
		// TODO:
		let third_party_investors = true;

		if self.visbility && third_party_investors {
			msg!("Publc index funds cannot be updated");
			return Ok(false);
		}
		self.visbility = new_visibility;
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
