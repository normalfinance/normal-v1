use std::fmt;
use std::fmt::{ Display, Formatter };

use anchor_lang::prelude::*;
use borsh::{ BorshDeserialize, BorshSerialize };

use crate::error::{ NormalResult, ErrorCode };
use crate::math::casting::Cast;
use crate::constants::constants::{
	AMM_RESERVE_PRECISION,
	FIVE_MINUTE,
	ONE_HOUR,
};
#[cfg(test)]
use crate::constants::constants::PRICE_PRECISION_I64;
use crate::math::safe_math::SafeMath;

use crate::math::stats::calculate_new_twap;
use crate::state::oracle::{ HistoricalIndexData, HistoricalOracleData };
use crate::state::insurance::InsuranceClaim;
use crate::state::paused_operations::{ Operation, InsuranceFundOperation };
use crate::state::traits::{ MarketIndexOffset, Size };
use crate::{ validate, PERCENTAGE_PRECISION };

use crate::state::amm::AMM;

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
pub enum MarketStatus {
	/// warm up period for initialization, fills are paused
	#[default]
	Initialized,
	/// all operations allowed
	Active,
	/// fills only able to reduce liability
	ReduceOnly,
	/// market has determined settlement price and positions are expired must be settled
	Settlement,
	/// market has no remaining participants
	Delisted,
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
pub enum SyntheticType {
	#[default]
	Asset,
	IndexFund,
	Yield,
}

#[derive(
	Clone,
	Copy,
	BorshSerialize,
	BorshDeserialize,
	PartialEq,
	Debug,
	Eq,
	PartialOrd,
	Ord,
	Default
)]
pub enum SyntheticTier {
	/// max insurance capped at A level
	A,
	/// max insurance capped at B level
	B,
	/// max insurance capped at C level
	C,
	/// no insurance
	Speculative,
	/// no insurance, another tranches below
	#[default]
	HighlySpeculative,
	/// no insurance, only single position allowed
	Isolated,
}

impl SyntheticTier {
	pub fn is_as_safe_as(&self, best_contract: &SyntheticTier) -> bool {
		self.is_as_safe_as_contract(best_contract)
	}

	pub fn is_as_safe_as_contract(&self, other: &SyntheticTier) -> bool {
		// Contract Tier A safest
		self <= other
	}
}

#[account(zero_copy(unsafe))]
#[derive(PartialEq, Eq, Debug)]
#[repr(C)]
pub struct Market {
	/// The address of the market. It is a pda of the market index
	pub pubkey: Pubkey,

	/// The automated market maker
	pub amm: Pubkey,

	/// Meta
	///
	pub market_index: u16,
	/// The encoded display name for the market e.g. SOL
	pub name: [u8; 32],
	pub status: MarketStatus,
	/// The asset tier affects how a deposit can be used as collateral and the priority for a borrow being liquidated
	pub synthetic_tier: SyntheticTier,
	/// The vault used to store the market's pnl and synthetic inventory
	pub vault: Pubkey,
	pub paused_operations: u8,
	/// The market's pnl pool. When users settle negative pnl, the balance increases.
	/// When users settle positive pnl, the balance decreases. Can not go negative.
	pub pnl_pool: PoolBalance,

	/// Token
	///
	/// The token mint of the market
	pub mint: Pubkey,
	/// The market's token mint's decimals. To from decimals to a precision, 10^decimals
	pub decimals: u32,
	pub token_program: u8,

	/// Oracle
	///
	pub historical_oracle_data: HistoricalOracleData,
	pub historical_index_data: HistoricalIndexData,

	/// Insurance
	///
	/// Details on the insurance fund covering bankruptcies in this markets token
	/// Covers bankruptcies for borrows with this markets token and perps settling in this markets token
	pub insurance_fund: Pubkey,
	/// The market's claim on the insurance fund
	pub insurance_claim: InsuranceClaim,

	/// Fees
	///
	/// Revenue the protocol has collected in the quote asset (i.e. SOL or USDC)
	pub fee_pool: PoolBalance,
	/// The total fees collected for this market
	/// precision: QUOTE_PRECISION
	pub total_fee: u128,
	/// The percentage of fees the insurance fund receives
	pub insurance_fund_fee_pct: u32,
	/// The percentage of fees used to burn gov tokens
	pub gov_token_burn_fee_pct: u32,
	pub fee_adjustment: i16,

	/// Settlement
	///
	/// The time the market is set to expire. Only set if market is in reduce only mode
	pub expiry_ts: i64,

	/// Order Config
	///
	/// Orders must be a multiple of the step size
	/// precision: token mint precision
	pub order_step_size: u64,
	/// Orders must be a multiple of the tick size
	/// precision: PRICE_PRECISION
	pub order_tick_size: u64,
	/// The minimum order size
	/// precision: token mint precision
	pub min_order_size: u64,
	/// The maximum position size
	/// if the limit is 0, there is no limit
	/// precision: token mint precision
	pub max_position_size: u64,
	/// Every trade has a fill record id. This is the next id to use
	pub next_fill_record_id: u64,

	// FROM PERP

	/// Stats
	///
	/// number of users in a position (base)
	pub number_of_users_with_base: u32,
	/// number of users in a position (pnl) or pnl (quote)
	pub number_of_users: u32,

	pub padding: [u8; 41],
}

impl Default for Market {
	fn default() -> Self {
		Market {
			pubkey: Pubkey::default(),

			amm: Pubkey::default(),
			mint: Pubkey::default(),
			vault: Pubkey::default(),
			name: [0; 32],
			historical_oracle_data: HistoricalOracleData::default(),
			historical_index_data: HistoricalIndexData::default(),

			pnl_pool: PoolBalance::default(),

			// Oracle

			// Insurance
			insurance_fund: InsuranceFund::default(),
			insurance_claim: InsuranceClaim::default(),

			// Fees
			fee_pool: PoolBalance::default(),
			total_fee: 0,
			insurance_fund_fee_pct: 0,
			gov_token_burn_fee_pct: 0,
			fee_adjustment: 0,

			expiry_ts: 0,
			order_step_size: 1,
			order_tick_size: 0,
			min_order_size: 0,
			max_position_size: 0,
			next_fill_record_id: 0,
			decimals: 0,
			market_index: 0,

			status: MarketStatus::default(),
			synthetic_tier: SyntheticTier::default(),
			paused_operations: 0,

			token_program: 0,
			// ...
			number_of_users_with_base: 0,
			number_of_users: 0,
			// ...
			padding: [0; 41],
		}
	}
}

impl Size for Market {
	const SIZE: usize = 776; // TODO: this needs to be updated to final market size
}

impl MarketIndexOffset for Market {
	const MARKET_INDEX_OFFSET: usize = 684;
}

impl Market {
	pub fn is_in_settlement(&self, now: i64) -> bool {
		let in_settlement = matches!(
			self.status,
			MarketStatus::Settlement | MarketStatus::Delisted
		);
		let expired = self.expiry_ts != 0 && now >= self.expiry_ts;
		in_settlement || expired
	}

	pub fn is_reduce_only(&self) -> NormalResult<bool> {
		Ok(self.status == MarketStatus::ReduceOnly)
	}

	pub fn is_operation_paused(&self, operation: Operation) -> bool {
		Operation::is_operation_paused(self.paused_operations, operation)
	}

	pub fn fills_enabled(&self) -> bool {
		matches!(self.status, MarketStatus::Active | MarketStatus::ReduceOnly) &&
			!self.is_operation_paused(Operation::Fill)
	}

	pub fn get_max_confidence_interval_multiplier(self) -> NormalResult<u64> {
		// assuming validity_guard_rails max confidence pct is 2%
		Ok(match self.synthetic_tier {
			SyntheticTier::A => 1, // 2%
			SyntheticTier::B => 1, // 2%
			SyntheticTier::C => 2, // 4%
			SyntheticTier::Speculative => 10, // 20%
			SyntheticTier::HighlySpeculative => 50, // 100%
			SyntheticTier::Isolated => 50, // 100%
		})
	}

	pub fn get_sanitize_clamp_denominator(self) -> NormalResult<Option<i64>> {
		Ok(match self.synthetic_tier {
			SyntheticTier::A => Some(10_i64), // 10%
			SyntheticTier::B => Some(5_i64), // 20%
			SyntheticTier::C => Some(2_i64), // 50%
			SyntheticTier::Speculative => None, // DEFAULT_MAX_TWAP_UPDATE_PRICE_BAND_DENOMINATOR
			SyntheticTier::HighlySpeculative => None, // DEFAULT_MAX_TWAP_UPDATE_PRICE_BAND_DENOMINATOR
			SyntheticTier::Isolated => None, // DEFAULT_MAX_TWAP_UPDATE_PRICE_BAND_DENOMINATOR
		})
	}

	// from spot

	pub fn get_precision(self) -> u64 {
		(10_u64).pow(self.decimals)
	}

	pub fn update_historical_index_price(
		&mut self,
		best_bid: Option<u64>,
		best_ask: Option<u64>,
		now: i64
	) -> NormalResult {
		let mut mid_price = 0;
		if let Some(best_bid) = best_bid {
			self.historical_index_data.last_index_bid_price = best_bid;
			mid_price += best_bid;
		}

		if let Some(best_ask) = best_ask {
			self.historical_index_data.last_index_ask_price = best_ask;
			mid_price = if mid_price == 0 {
				best_ask
			} else {
				mid_price.safe_add(best_ask)?.safe_div(2)?
			};
		}

		self.historical_index_data.last_index_price_twap = calculate_new_twap(
			mid_price.cast()?,
			now,
			self.historical_index_data.last_index_price_twap.cast()?,
			self.historical_index_data.last_index_price_twap_ts,
			ONE_HOUR
		)?.cast()?;

		self.historical_index_data.last_index_price_twap_5min = calculate_new_twap(
			mid_price.cast()?,
			now,
			self.historical_index_data.last_index_price_twap_5min.cast()?,
			self.historical_index_data.last_index_price_twap_ts,
			FIVE_MINUTE as i64
		)?.cast()?;

		self.historical_index_data.last_index_price_twap_ts = now;

		Ok(())
	}

	// from perp

	pub fn get_auction_end_min_max_divisors(self) -> NormalResult<(u64, u64)> {
		Ok(match self.synthetic_tier {
			SyntheticTier::A => (1000, 50), // 10 bps, 2%
			SyntheticTier::B => (1000, 20), // 10 bps, 5%
			SyntheticTier::C => (500, 20), // 50 bps, 5%
			SyntheticTier::Speculative => (100, 10), // 1%, 10%
			SyntheticTier::HighlySpeculative => (50, 5), // 2%, 20%
			SyntheticTier::Isolated => (50, 5), // 2%, 20%
		})
	}

	pub fn get_open_interest(
		&self,
		base_oracle_price: i64,
		quote_oracle_price: i64
	) -> u128 {
		// open interest = value of quote - value of base

		// self.amm.

		let base_value = self.amm.base_asset_amount_long
			.abs()
			.unsigned_abs()
			.safe_mul(base_oracle_price)?;
		let quote_value = self.amm.quote_asset_amount
			.abs()
			.unsigned_abs()
			.safe_mul(quote_oracle_price)?;

		base_value.safe_sub(base_value)
	}

	pub fn get_market_depth(&self) -> NormalResult<u64> {
		// base amount used on user orders for funding calculation

		let open_interest = self.get_open_interest();

		let depth = open_interest
			.safe_div(1000)?
			.cast::<u64>()?
			.clamp(
				self.amm.min_order_size.safe_mul(100)?,
				self.amm.min_order_size.safe_mul(5000)?
			);

		Ok(depth)
	}

	pub fn update_market_with_counterparty(
		&mut self,
		delta: &PositionDelta,
		new_settled_base_asset_amount: i64
	) -> NormalResult {
		// indicates that position delta is settling lp counterparty
		if delta.remainder_base_asset_amount.is_some() {
			// todo: name for this is confusing, but adding is correct as is
			// definition: net position of users in the market that has the LP as a counterparty (which have NOT settled)
			self.amm.base_asset_amount_with_unsettled_lp =
				self.amm.base_asset_amount_with_unsettled_lp.safe_add(
					new_settled_base_asset_amount.cast()?
				)?;

			self.amm.quote_asset_amount_with_unsettled_lp =
				self.amm.quote_asset_amount_with_unsettled_lp.safe_add(
					delta.quote_asset_amount.cast()?
				)?;
		}

		Ok(())
	}

	pub fn is_price_divergence_ok_for_settle_pnl(
		&self,
		oracle_price: i64
	) -> NormalResult<bool> {
		let oracle_divergence = oracle_price
			.safe_sub(self.amm.historical_oracle_data.last_oracle_price_twap_5min)?
			.safe_mul(PERCENTAGE_PRECISION_I64)?
			.safe_div(
				self.amm.historical_oracle_data.last_oracle_price_twap_5min.min(
					oracle_price
				)
			)?
			.unsigned_abs();

		let oracle_divergence_limit = match self.synthetic_tier {
			SyntheticTier::A => PERCENTAGE_PRECISION_U64 / 200, // 50 bps
			SyntheticTier::B => PERCENTAGE_PRECISION_U64 / 200, // 50 bps
			SyntheticTier::C => PERCENTAGE_PRECISION_U64 / 100, // 100 bps
			SyntheticTier::Speculative => PERCENTAGE_PRECISION_U64 / 40, // 250 bps
			SyntheticTier::HighlySpeculative => PERCENTAGE_PRECISION_U64 / 40, // 250 bps
			SyntheticTier::Isolated => PERCENTAGE_PRECISION_U64 / 40, // 250 bps
		};

		if oracle_divergence >= oracle_divergence_limit {
			msg!(
				"market_index={} price divergence too large to safely settle pnl: {} >= {}",
				self.market_index,
				oracle_divergence,
				oracle_divergence_limit
			);
			return Ok(false);
		}

		let min_price = oracle_price.min(
			self.amm.historical_oracle_data.last_oracle_price_twap_5min
		);

		let std_limit = (
			match self.synthetic_tier {
				SyntheticTier::A => min_price / 50, // 200 bps
				SyntheticTier::B => min_price / 50, // 200 bps
				SyntheticTier::C => min_price / 20, // 500 bps
				SyntheticTier::Speculative => min_price / 10, // 1000 bps
				SyntheticTier::HighlySpeculative => min_price / 10, // 1000 bps
				SyntheticTier::Isolated => min_price / 10, // 1000 bps
			}
		).unsigned_abs();

		if self.amm.oracle_std.max(self.amm.mark_std) >= std_limit {
			msg!(
				"market_index={} std too large to safely settle pnl: {} >= {}",
				self.market_index,
				self.amm.oracle_std.max(self.amm.mark_std),
				std_limit
			);
			return Ok(false);
		}

		Ok(true)
	}

	pub fn can_sanitize_market_order_auctions(&self) -> bool {
		true
	}

	pub fn is_index_fund_market(&self) -> bool {
		self.SyntheticType == SyntheticType::IndexFund
	}

	pub fn is_yield_market(&self) -> bool {
		self.SyntheticType == SyntheticType::Yield
	}
}

#[cfg(test)]
impl Market {
	pub fn default_base_market() -> Self {
		Market {
			market_index: 1,
			decimals: 9,
			order_step_size: 1,
			order_tick_size: 1,
			status: MarketStatus::Active,
			..Market::default()
		}
	}

	pub fn default_quote_market() -> Self {
		Market {
			decimals: 6,
			order_tick_size: 1,
			status: MarketStatus::Active,
			historical_oracle_data: HistoricalOracleData {
				last_oracle_price_twap: PRICE_PRECISION_I64,
				last_oracle_price_twap_5min: PRICE_PRECISION_I64,
				..HistoricalOracleData::default()
			},
			..Market::default()
		}
	}
}

#[derive(
	Clone,
	Copy,
	BorshSerialize,
	BorshDeserialize,
	PartialEq,
	Eq,
	Debug,
	Default
)]
pub enum BalanceType {
	#[default]
	Deposit,
	Borrow,
}

impl Display for BalanceType {
	fn fmt(&self, f: &mut Formatter) -> fmt::Result {
		match self {
			BalanceType::Deposit => write!(f, "BalanceType::Deposit"),
			BalanceType::Borrow => write!(f, "BalanceType::Borrow"),
		}
	}
}

pub trait Balance {
	fn market_index(&self) -> u16;

	fn balance_type(&self) -> &BalanceType;

	fn balance(&self) -> u128;

	fn increase_balance(&mut self, delta: u128) -> NormalResult;

	fn decrease_balance(&mut self, delta: u128) -> NormalResult;

	fn update_balance_type(&mut self, balance_type: BalanceType) -> NormalResult;
}

#[zero_copy(unsafe)]
#[derive(Default, Eq, PartialEq, Debug)]
#[repr(C)]
pub struct PoolBalance {
	/// precision: SPOT_BALANCE_PRECISION
	pub balance: u128,
	/// The market the pool is for
	pub market_index: u16,
	pub padding: [u8; 6],
}

impl Balance for PoolBalance {
	fn market_index(&self) -> u16 {
		self.market_index
	}

	fn balance_type(&self) -> &BalanceType {
		&BalanceType::Deposit
	}

	fn balance(&self) -> u128 {
		self.balance
	}

	fn increase_balance(&mut self, delta: u128) -> NormalResult {
		self.balance = self.balance.safe_add(delta)?;
		Ok(())
	}

	fn decrease_balance(&mut self, delta: u128) -> NormalResult {
		self.balance = self.balance.safe_sub(delta)?;
		Ok(())
	}

	fn update_balance_type(
		&mut self,
		_balance_type: BalanceType
	) -> NormalResult {
		Err(ErrorCode::CantUpdatePoolBalanceType)
	}
}
