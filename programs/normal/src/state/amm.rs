use anchor_lang::prelude::*;

use anchor_lang::prelude::*;

use std::cmp::max;

use crate::controller::position::{ PositionDelta, PositionDirection };
use crate::error::{ NormalResult, ErrorCode };
use crate::math::amm;
use crate::math::casting::Cast;
#[cfg(test)]
use crate::math::constants::{
	AMM_RESERVE_PRECISION,
	MAX_CONCENTRATION_COEFFICIENT,
	PRICE_PRECISION_I64,
};
use crate::math::constants::{
	AMM_RESERVE_PRECISION_I128,
	AMM_TO_QUOTE_PRECISION_RATIO,
	BID_ASK_SPREAD_PRECISION,
	BID_ASK_SPREAD_PRECISION_U128,
	LP_FEE_SLICE_DENOMINATOR,
	LP_FEE_SLICE_NUMERATOR,
	PEG_PRECISION,
	PERCENTAGE_PRECISION,
	PERCENTAGE_PRECISION_I128,
	PERCENTAGE_PRECISION_I64,
	PERCENTAGE_PRECISION_U64,
	PRICE_PRECISION,
	TWENTY_FOUR_HOUR,
};
use crate::math::helpers::get_proportion_i128;
use crate::math::margin::{
	calculate_size_discount_asset_weight,
	calculate_size_premium_liability_weight,
	MarginRequirementType,
};
use crate::math::safe_math::SafeMath;
use crate::math::stats;
use crate::state::events::OrderActionExplanation;
use num_integer::Roots;

use crate::state::oracle::{ HistoricalOracleData, OracleSource };
use crate::state::market::PoolBalance;
use crate::controller::position::{ PositionDirection, PositionDelta };

// use normal_macros::assert_no_slop;

#[derive(
	Clone,
	Copy,
	BorshSerialize,
	BorshDeserialize,
	PartialEq,
	Debug,
	Eq,
	PartialOrd,
	Ord
)]
pub enum AMMLiquiditySplit {
	ProtocolOwned,
	LPOwned,
	Shared,
}

impl AMMLiquiditySplit {
	pub fn get_order_action_explanation(&self) -> OrderActionExplanation {
		match &self {
			AMMLiquiditySplit::ProtocolOwned =>
				OrderActionExplanation::OrderFilledWithAMMJit,
			AMMLiquiditySplit::LPOwned =>
				OrderActionExplanation::OrderFilledWithLPJit,
			AMMLiquiditySplit::Shared =>
				OrderActionExplanation::OrderFilledWithAMMJitLPSplit,
		}
	}
}

#[assert_no_slop]
#[zero_copy(unsafe)]
#[derive(Debug, PartialEq, Eq)]
#[repr(C)]
pub struct AMM {
	/// Token Synth
	pub token: Pubkey,
	/// Mint information for token Synth
	pub token_mint: Pubkey,

	/// accumulated base asset amount since inception per lp share
	/// precision: QUOTE_PRECISION
	pub base_asset_amount_per_lp: i128,
	/// accumulated quote asset amount since inception per lp share
	/// precision: QUOTE_PRECISION
	pub quote_asset_amount_per_lp: i128,

	/// Oracle
	///
	/// oracle price data public key
	pub oracle: Pubkey,
	/// stores historically witnessed oracle data
	pub historical_oracle_data: HistoricalOracleData,
	/// the oracle provider information. used to decode/scale the oracle public key
	pub oracle_source: OracleSource,
	/// the pct size of the oracle confidence interval
	/// precision: PERCENTAGE_PRECISION
	pub last_oracle_conf_pct: u64,
	/// tracks whether the oracle was considered valid at the last AMM update
	pub last_oracle_valid: bool,

	/// Base Reserve (Synthetic)
	///
	/// `x` reserves for constant product mm formula (x * y = k)
	/// precision: AMM_RESERVE_PRECISION
	pub base_asset_reserve: u128,
	/// transformed base_asset_reserve for users going long
	/// precision: AMM_RESERVE_PRECISION
	pub ask_base_asset_reserve: u128,
	/// transformed base_asset_reserve for users going short
	/// precision: AMM_RESERVE_PRECISION
	pub bid_base_asset_reserve: u128,

	/// Quote Reserve (SOL and USDC)
	///
	/// `y` reserves for constant product mm formula (x * y = k)
	/// precision: AMM_RESERVE_PRECISION
	pub quote_asset_reserve: u128,
	/// transformed quote_asset_reserve for users going long
	/// precision: AMM_RESERVE_PRECISION
	pub ask_quote_asset_reserve: u128,
	/// transformed quote_asset_reserve for users going short
	/// precision: AMM_RESERVE_PRECISION
	pub bid_quote_asset_reserve: u128,

	/// determines how close the min/max base asset reserve sit vs base reserves
	/// allow for decreasing slippage without increasing liquidity and v.v.
	/// precision: PERCENTAGE_PRECISION
	pub concentration_coef: u128,
	/// minimum base_asset_reserve allowed before AMM is unavailable
	/// precision: AMM_RESERVE_PRECISION
	pub min_base_asset_reserve: u128,
	/// maximum base_asset_reserve allowed before AMM is unavailable
	/// precision: AMM_RESERVE_PRECISION
	pub max_base_asset_reserve: u128,
	/// `sqrt(k)` in constant product mm formula (x * y = k). stored to avoid drift caused by integer math issues
	/// precision: AMM_RESERVE_PRECISION
	pub sqrt_k: u128,
	/// normalizing numerical factor for y, its use offers lowest slippage in cp-curve when market is balanced
	/// precision: PEG_PRECISION
	pub peg_multiplier: u128,
	/// y when market is balanced. stored to save computation
	/// precision: AMM_RESERVE_PRECISION
	pub terminal_quote_asset_reserve: u128,

	/// Base Asset (Synthetic)
	///
	/// always non-negative. tracks number of total longs in market (regardless of counterparty)
	/// precision: BASE_PRECISION
	pub base_asset_amount_long: i128,
	/// always non-positive. tracks number of total shorts in market (regardless of counterparty)
	/// precision: BASE_PRECISION
	pub base_asset_amount_short: i128,
	/// tracks net position (longs-shorts) in market with AMM as counterparty
	/// precision: BASE_PRECISION
	pub base_asset_amount_with_amm: i128,
	/// tracks net position (longs-shorts) in market with LPs as counterparty
	/// precision: BASE_PRECISION
	pub base_asset_amount_with_unsettled_lp: i128,

	/// max allowed open interest, blocks trades that breach this value
	/// precision: BASE_PRECISION
	pub max_open_interest: u128,

	/// Quote Asset (SOL and USDC)
	///
	/// sum of all user's quote_asset_amount in market
	/// precision: QUOTE_PRECISION
	pub quote_asset_amount: i128,
	/// sum of all long user's quote_entry_amount in market
	/// precision: QUOTE_PRECISION
	pub quote_entry_amount_long: i128,
	/// sum of all short user's quote_entry_amount in market
	/// precision: QUOTE_PRECISION
	pub quote_entry_amount_short: i128,
	/// sum of all long user's quote_break_even_amount in market
	/// precision: QUOTE_PRECISION
	pub quote_break_even_amount_long: i128,
	/// sum of all short user's quote_break_even_amount in market
	/// precision: QUOTE_PRECISION
	pub quote_break_even_amount_short: i128,

	/// total user lp shares of sqrt_k (protocol owned liquidity = sqrt_k - last_funding_rate)
	/// precision: AMM_RESERVE_PRECISION
	pub user_lp_shares: u128,

	/// Fees
	///
	pub fee_pool: PoolBalance,
	/// total fees collected by this market
	/// precision: QUOTE_PRECISION
	pub total_fee: i128,
	/// total fees collected by the vAMM's bid/ask spread
	/// precision: QUOTE_PRECISION
	pub total_mm_fee: i128,
	/// total fees collected by exchange fee schedule
	/// precision: QUOTE_PRECISION
	pub total_exchange_fee: u128,
	/// total fees minus any recognized upnl and pool withdraws
	/// precision: QUOTE_PRECISION
	pub total_fee_minus_distributions: i128,
	/// sum of all fees from fee pool withdrawn to revenue pool
	/// precision: QUOTE_PRECISION
	pub total_fee_withdrawn: u128,

	/// accumulated social loss paid by users since inception in market
	pub total_social_loss: u128,

	/// the last seen oracle price partially shrunk toward the amm reserve price
	/// precision: PRICE_PRECISION
	pub last_oracle_normalised_price: i64,
	/// the gap between the oracle price and the reserve price = y * peg_multiplier / x
	pub last_oracle_reserve_price_spread_pct: i64,
	/// average estimate of bid price over funding_period
	/// precision: PRICE_PRECISION
	pub last_bid_price_twap: u64,
	/// average estimate of ask price over funding_period
	/// precision: PRICE_PRECISION
	pub last_ask_price_twap: u64,
	/// average estimate of (bid+ask)/2 price over funding_period
	/// precision: PRICE_PRECISION
	pub last_mark_price_twap: u64,
	/// average estimate of (bid+ask)/2 price over FIVE_MINUTES
	pub last_mark_price_twap_5min: u64,
	/// the last blockchain slot the amm was updated
	pub last_update_slot: u64,

	/// the base step size (increment) of orders
	/// precision: BASE_PRECISION
	pub order_step_size: u64,
	/// the price tick size of orders
	/// precision: PRICE_PRECISION
	pub order_tick_size: u64,
	/// the minimum base size of an order
	/// precision: BASE_PRECISION
	pub min_order_size: u64,
	/// the max base size a single user can have
	/// precision: BASE_PRECISION
	pub max_position_size: u64,

	/// Volume
	///
	/// estimated total of volume in market
	/// QUOTE_PRECISION
	pub volume_24h: u64,
	/// the volume intensity of long fills against AMM
	pub long_intensity_volume: u64,
	/// the volume intensity of short fills against AMM
	pub short_intensity_volume: u64,
	/// the count intensity of long fills against AMM
	pub long_intensity_count: u32,
	/// the count intensity of short fills against AMM
	pub short_intensity_count: u32,

	/// the blockchain unix timestamp at the time of the last trade
	pub last_trade_ts: i64,
	/// estimate of standard deviation of the fill (mark) prices
	/// precision: PRICE_PRECISION
	pub mark_std: u64,
	/// estimate of standard deviation of the oracle price at each update
	/// precision: PRICE_PRECISION
	pub oracle_std: u64,
	/// the last unix_timestamp the mark twap was updated
	pub last_mark_price_twap_ts: i64,

	/// Spread
	///
	/// the minimum spread the AMM can quote. also used as step size for some spread logic increases.
	pub base_spread: u32,
	/// the maximum spread the AMM can quote
	pub max_spread: u32,
	/// the spread for asks vs the reserve price
	pub long_spread: u32,
	/// the spread for bids vs the reserve price
	pub short_spread: u32,

	/// the fraction of total available liquidity a single fill on the AMM can consume
	pub max_fill_reserve_fraction: u16,
	/// the maximum slippage a single fill on the AMM can push
	pub max_slippage_ratio: u16,
	/// the update intensity of AMM formulaic updates (adjusting k). 0-100
	pub curve_update_intensity: u8,
	/// the jit intensity of AMM. larger intensity means larger participation in jit. 0 means no jit participation.
	/// (0, 100] is intensity for protocol-owned AMM. (100, 200] is intensity for user LP-owned AMM.
	pub amm_jit_intensity: u8,

	/// the target value for `base_asset_amount_per_lp`, used during AMM JIT with LP split
	/// precision: BASE_PRECISION
	pub target_base_asset_amount_per_lp: i32,
	/// expo for unit of per_lp, base 10 (if per_lp_base=X, then per_lp unit is 10^X)
	pub per_lp_base: i8,
	pub padding1: u8,
	pub padding2: u16,
	pub total_fee_earned_per_lp: u64,
	pub quote_asset_amount_with_unsettled_lp: i64,
	pub reference_price_offset: i32,
	pub padding: [u8; 12],
}

impl Default for AMM {
	fn default() -> Self {
		AMM {
			token: 0,
			token_mint: 0,
			oracle: Pubkey::default(),
			historical_oracle_data: HistoricalOracleData::default(),
			base_asset_amount_per_lp: 0,
			quote_asset_amount_per_lp: 0,
			fee_pool: PoolBalance::default(),
			base_asset_reserve: 0,
			quote_asset_reserve: 0,
			concentration_coef: 0,
			min_base_asset_reserve: 0,
			max_base_asset_reserve: 0,
			sqrt_k: 0,
			peg_multiplier: 0,
			terminal_quote_asset_reserve: 0,
			base_asset_amount_long: 0,
			base_asset_amount_short: 0,
			base_asset_amount_with_amm: 0,
			base_asset_amount_with_unsettled_lp: 0,
			max_open_interest: 0,
			quote_asset_amount: 0,
			quote_entry_amount_long: 0,
			quote_entry_amount_short: 0,
			quote_break_even_amount_long: 0,
			quote_break_even_amount_short: 0,
			user_lp_shares: 0,
			total_fee: 0,
			total_mm_fee: 0,
			total_exchange_fee: 0,
			total_fee_minus_distributions: 0,
			total_fee_withdrawn: 0,
			total_social_loss: 0,
			ask_base_asset_reserve: 0,
			ask_quote_asset_reserve: 0,
			bid_base_asset_reserve: 0,
			bid_quote_asset_reserve: 0,
			last_oracle_normalised_price: 0,
			last_oracle_reserve_price_spread_pct: 0,
			last_bid_price_twap: 0,
			last_ask_price_twap: 0,
			last_mark_price_twap: 0,
			last_mark_price_twap_5min: 0,
			last_update_slot: 0,
			last_oracle_conf_pct: 0,
			order_step_size: 0,
			order_tick_size: 0,
			min_order_size: 1,
			max_position_size: 0,
			volume_24h: 0,
			long_intensity_volume: 0,
			short_intensity_volume: 0,
			last_trade_ts: 0,
			mark_std: 0,
			oracle_std: 0,
			last_mark_price_twap_ts: 0,
			base_spread: 0,
			max_spread: 0,
			long_spread: 0,
			short_spread: 0,
			long_intensity_count: 0,
			short_intensity_count: 0,
			max_fill_reserve_fraction: 0,
			max_slippage_ratio: 0,
			curve_update_intensity: 0,
			amm_jit_intensity: 0,
			oracle_source: OracleSource::default(),
			last_oracle_valid: false,
			target_base_asset_amount_per_lp: 0,
			per_lp_base: 0,
			padding1: 0,
			padding2: 0,
			total_fee_earned_per_lp: 0,
			quote_asset_amount_with_unsettled_lp: 0,
			reference_price_offset: 0,
			padding: [0; 12],
		}
	}
}

impl AMM {
	pub fn get_fallback_price(
		self,
		direction: &PositionDirection,
		amm_available_liquidity: u64,
		oracle_price: i64,
		seconds_til_order_expiry: i64
	) -> NormalResult<u64> {
		// PRICE_PRECISION
		if direction.eq(&PositionDirection::Long) {
			// pick amm ask + buffer if theres liquidity
			// otherwise be aggressive vs oracle + 1hr premium
			if amm_available_liquidity >= self.min_order_size {
				let reserve_price = self.reserve_price()?;
				let amm_ask_price: i64 = self.ask_price(reserve_price)?.cast()?;
				amm_ask_price
					.safe_add(
						amm_ask_price / (seconds_til_order_expiry * 20).clamp(100, 200)
					)?
					.cast::<u64>()
			} else {
				oracle_price
					.safe_add(
						self.last_ask_price_twap
							.cast::<i64>()?
							.safe_sub(self.historical_oracle_data.last_oracle_price_twap)?
							.max(0)
					)?
					.safe_add(
						oracle_price / (seconds_til_order_expiry * 2).clamp(10, 50)
					)?
					.cast::<u64>()
			}
		} else {
			// pick amm bid - buffer if theres liquidity
			// otherwise be aggressive vs oracle + 1hr bid premium
			if amm_available_liquidity >= self.min_order_size {
				let reserve_price = self.reserve_price()?;
				let amm_bid_price: i64 = self.bid_price(reserve_price)?.cast()?;
				amm_bid_price
					.safe_sub(
						amm_bid_price / (seconds_til_order_expiry * 20).clamp(100, 200)
					)?
					.cast::<u64>()
			} else {
				oracle_price
					.safe_add(
						self.last_bid_price_twap
							.cast::<i64>()?
							.safe_sub(self.historical_oracle_data.last_oracle_price_twap)?
							.min(0)
					)?
					.safe_sub(
						oracle_price / (seconds_til_order_expiry * 2).clamp(10, 50)
					)?
					.max(0)
					.cast::<u64>()
			}
		}
	}

	pub fn get_lower_bound_sqrt_k(self) -> NormalResult<u128> {
		Ok(
			self.sqrt_k.min(
				self.user_lp_shares
					.safe_add(self.user_lp_shares.safe_div(1000)?)?
					.max(self.min_order_size.cast()?)
					.max(self.base_asset_amount_with_amm.unsigned_abs().cast()?)
			)
		)
	}

	pub fn get_protocol_owned_position(self) -> NormalResult<i64> {
		self.base_asset_amount_with_amm
			.safe_add(self.base_asset_amount_with_unsettled_lp)?
			.cast::<i64>()
	}

	pub fn get_max_reference_price_offset(self) -> NormalResult<i64> {
		if self.curve_update_intensity <= 100 {
			return Ok(0);
		}

		let lower_bound_multiplier: i64 = self.curve_update_intensity
			.safe_sub(100)?
			.cast::<i64>()?;

		// always allow 1-100 bps of price offset, up to a fifth of the market's max_spread
		let lb_bps = (PERCENTAGE_PRECISION.cast::<i64>()? / 10000).safe_mul(
			lower_bound_multiplier
		)?;
		let max_offset = (self.max_spread.cast::<i64>()? / 5).max(lb_bps);

		Ok(max_offset)
	}

	pub fn get_per_lp_base_unit(self) -> NormalResult<i128> {
		let scalar: i128 = (10_i128).pow(self.per_lp_base.abs().cast()?);

		if self.per_lp_base > 0 {
			AMM_RESERVE_PRECISION_I128.safe_mul(scalar)
		} else {
			AMM_RESERVE_PRECISION_I128.safe_div(scalar)
		}
	}

	pub fn calculate_lp_base_delta(
		&self,
		per_lp_delta_base: i128,
		base_unit: i128
	) -> NormalResult<i128> {
		// calculate dedicated for user lp shares
		let lp_delta_base = get_proportion_i128(
			per_lp_delta_base,
			self.user_lp_shares,
			base_unit.cast()?
		)?;

		Ok(lp_delta_base)
	}

	pub fn calculate_per_lp_delta(
		&self,
		delta: &PositionDelta,
		fee_to_market: i128,
		liquidity_split: AMMLiquiditySplit,
		base_unit: i128
	) -> NormalResult<(i128, i128, i128)> {
		let total_lp_shares = if liquidity_split == AMMLiquiditySplit::LPOwned {
			self.user_lp_shares
		} else {
			self.sqrt_k
		};

		// update Market per lp position
		let per_lp_delta_base = get_proportion_i128(
			delta.base_asset_amount.cast()?,
			base_unit.cast()?,
			total_lp_shares //.safe_div_ceil(rebase_divisor.cast()?)?,
		)?;

		let mut per_lp_delta_quote = get_proportion_i128(
			delta.quote_asset_amount.cast()?,
			base_unit.cast()?,
			total_lp_shares //.safe_div_ceil(rebase_divisor.cast()?)?,
		)?;

		// user position delta is short => lp position delta is long
		if per_lp_delta_base < 0 {
			// add one => lp subtract 1
			per_lp_delta_quote = per_lp_delta_quote.safe_add(1)?;
		}

		// 1/5 of fee auto goes to market
		// the rest goes to lps/market proportional
		let per_lp_fee: i128 = if fee_to_market > 0 {
			get_proportion_i128(
				fee_to_market,
				LP_FEE_SLICE_NUMERATOR,
				LP_FEE_SLICE_DENOMINATOR
			)?
				.safe_mul(base_unit)?
				.safe_div(total_lp_shares.cast::<i128>()?)?
		} else {
			0
		};

		Ok((per_lp_delta_base, per_lp_delta_quote, per_lp_fee))
	}

	pub fn get_target_base_asset_amount_per_lp(&self) -> NormalResult<i128> {
		if self.target_base_asset_amount_per_lp == 0 {
			return Ok(0_i128);
		}

		let target_base_asset_amount_per_lp: i128 = if self.per_lp_base > 0 {
			let rebase_divisor = (10_i128).pow(self.per_lp_base.abs().cast()?);
			self.target_base_asset_amount_per_lp
				.cast::<i128>()?
				.safe_mul(rebase_divisor)?
		} else if self.per_lp_base < 0 {
			let rebase_divisor = (10_i128).pow(self.per_lp_base.abs().cast()?);
			self.target_base_asset_amount_per_lp
				.cast::<i128>()?
				.safe_div(rebase_divisor)?
		} else {
			self.target_base_asset_amount_per_lp.cast::<i128>()?
		};

		Ok(target_base_asset_amount_per_lp)
	}

	pub fn imbalanced_base_asset_amount_with_lp(&self) -> NormalResult<i128> {
		let target_lp_gap = self.base_asset_amount_per_lp.safe_sub(
			self.get_target_base_asset_amount_per_lp()?
		)?;

		let base_unit = self.get_per_lp_base_unit()?.cast()?;

		get_proportion_i128(target_lp_gap, self.user_lp_shares, base_unit)
	}

	pub fn amm_wants_to_jit_make(
		&self,
		taker_direction: PositionDirection
	) -> NormalResult<bool> {
		let amm_wants_to_jit_make = match taker_direction {
			PositionDirection::Long => {
				self.base_asset_amount_with_amm < -self.order_step_size.cast()?
			}
			PositionDirection::Short => {
				self.base_asset_amount_with_amm > self.order_step_size.cast()?
			}
		};
		Ok(amm_wants_to_jit_make && self.amm_jit_is_active())
	}

	pub fn amm_lp_wants_to_jit_make(
		&self,
		taker_direction: PositionDirection
	) -> NormalResult<bool> {
		if self.user_lp_shares == 0 {
			return Ok(false);
		}

		let amm_lp_wants_to_jit_make = match taker_direction {
			PositionDirection::Long => {
				self.base_asset_amount_per_lp >
					self.get_target_base_asset_amount_per_lp()?
			}
			PositionDirection::Short => {
				self.base_asset_amount_per_lp <
					self.get_target_base_asset_amount_per_lp()?
			}
		};
		Ok(amm_lp_wants_to_jit_make && self.amm_lp_jit_is_active())
	}

	pub fn amm_lp_allowed_to_jit_make(
		&self,
		amm_wants_to_jit_make: bool
	) -> NormalResult<bool> {
		// only allow lps to make when the amm inventory is below a certain level of available liquidity
		// i.e. 10%
		if amm_wants_to_jit_make {
			// inventory scale
			let (max_bids, max_asks) = amm::_calculate_market_open_bids_asks(
				self.base_asset_reserve,
				self.min_base_asset_reserve,
				self.max_base_asset_reserve
			)?;

			let min_side_liquidity = max_bids.min(max_asks.abs());
			let protocol_owned_min_side_liquidity = get_proportion_i128(
				min_side_liquidity,
				self.sqrt_k.safe_sub(self.user_lp_shares)?,
				self.sqrt_k
			)?;

			Ok(
				self.base_asset_amount_with_amm.abs() <
					protocol_owned_min_side_liquidity.safe_div(10)?
			)
		} else {
			Ok(true)
		}
	}

	pub fn amm_jit_is_active(&self) -> bool {
		self.amm_jit_intensity > 0
	}

	pub fn amm_lp_jit_is_active(&self) -> bool {
		self.amm_jit_intensity > 100
	}

	pub fn reserve_price(&self) -> NormalResult<u64> {
		amm::calculate_price(
			self.quote_asset_reserve,
			self.base_asset_reserve,
			self.peg_multiplier
		)
	}

	pub fn bid_price(&self, reserve_price: u64) -> NormalResult<u64> {
		reserve_price
			.cast::<u128>()?
			.safe_mul(
				BID_ASK_SPREAD_PRECISION_U128.safe_sub(self.short_spread.cast()?)?
			)?
			.safe_div(BID_ASK_SPREAD_PRECISION_U128)?
			.cast()
	}

	pub fn ask_price(&self, reserve_price: u64) -> NormalResult<u64> {
		reserve_price
			.cast::<u128>()?
			.safe_mul(
				BID_ASK_SPREAD_PRECISION_U128.safe_add(self.long_spread.cast()?)?
			)?
			.safe_div(BID_ASK_SPREAD_PRECISION_U128)?
			.cast::<u64>()
	}

	pub fn bid_ask_price(&self, reserve_price: u64) -> NormalResult<(u64, u64)> {
		let bid_price = self.bid_price(reserve_price)?;
		let ask_price = self.ask_price(reserve_price)?;
		Ok((bid_price, ask_price))
	}

	pub fn last_ask_premium(&self) -> NormalResult<i64> {
		let reserve_price = self.reserve_price()?;
		let ask_price = self.ask_price(reserve_price)?.cast::<i64>()?;
		ask_price.safe_sub(self.historical_oracle_data.last_oracle_price)
	}

	pub fn last_bid_discount(&self) -> NormalResult<i64> {
		let reserve_price = self.reserve_price()?;
		let bid_price = self.bid_price(reserve_price)?.cast::<i64>()?;
		self.historical_oracle_data.last_oracle_price.safe_sub(bid_price)
	}

	pub fn can_lower_k(&self) -> NormalResult<bool> {
		let (max_bids, max_asks) = amm::calculate_market_open_bids_asks(self)?;
		let can_lower =
			self.base_asset_amount_with_amm.unsigned_abs() <
				max_bids.unsigned_abs().min(max_asks.unsigned_abs()) &&
			self.base_asset_amount_with_amm.unsigned_abs() <
				self.sqrt_k.safe_sub(self.user_lp_shares)?;
		Ok(can_lower)
	}

	pub fn get_oracle_twap(
		&self,
		price_oracle: &AccountInfo,
		slot: u64
	) -> NormalResult<Option<i64>> {
		match self.oracle_source {
			OracleSource::Pyth | OracleSource::PythStableCoin => {
				Ok(Some(self.get_pyth_twap(price_oracle, 1, false)?))
			}
			OracleSource::Pyth1K =>
				Ok(Some(self.get_pyth_twap(price_oracle, 1000, false)?)),
			OracleSource::Pyth1M =>
				Ok(Some(self.get_pyth_twap(price_oracle, 1000000, false)?)),
			OracleSource::Switchboard =>
				Ok(Some(get_switchboard_price(price_oracle, slot)?.price)),
			OracleSource::SwitchboardOnDemand => {
				Ok(Some(get_sb_on_demand_price(price_oracle, slot)?.price))
			}
			OracleSource::QuoteAsset => {
				msg!("Can't get oracle twap for quote asset");
				Err(ErrorCode::DefaultError)
			}
			OracleSource::PythPull | OracleSource::PythStableCoinPull => {
				Ok(Some(self.get_pyth_twap(price_oracle, 1, true)?))
			}
			OracleSource::Pyth1KPull =>
				Ok(Some(self.get_pyth_twap(price_oracle, 1000, true)?)),
			OracleSource::Pyth1MPull => {
				Ok(Some(self.get_pyth_twap(price_oracle, 1000000, true)?))
			}
		}
	}

	pub fn get_pyth_twap(
		&self,
		price_oracle: &AccountInfo,
		multiple: u128,
		is_pull_oracle: bool
	) -> NormalResult<i64> {
		let mut pyth_price_data: &[u8] = &price_oracle
			.try_borrow_data()
			.or(Err(ErrorCode::UnableToLoadOracle))?;

		let oracle_price: i64;
		let oracle_twap: i64;
		let oracle_exponent: i32;

		if is_pull_oracle {
			let price_message = pyth_solana_receiver_sdk::price_update::PriceUpdateV2
				::try_deserialize(&mut pyth_price_data)
				.or(Err(crate::error::ErrorCode::UnableToLoadOracle))?;
			oracle_price = price_message.price_message.price;
			oracle_twap = price_message.price_message.ema_price;
			oracle_exponent = price_message.price_message.exponent;
		} else {
			let price_data = pyth_client::cast::<pyth_client::Price>(pyth_price_data);
			oracle_price = price_data.agg.price;
			oracle_twap = price_data.twap.val;
			oracle_exponent = price_data.expo;
		}

		assert!(oracle_twap > oracle_price / 10);

		let oracle_precision = (10_u128)
			.pow(oracle_exponent.unsigned_abs())
			.safe_div(multiple)?;

		let mut oracle_scale_mult = 1;
		let mut oracle_scale_div = 1;

		if oracle_precision > PRICE_PRECISION {
			oracle_scale_div = oracle_precision.safe_div(PRICE_PRECISION)?;
		} else {
			oracle_scale_mult = PRICE_PRECISION.safe_div(oracle_precision)?;
		}

		oracle_twap
			.cast::<i128>()?
			.safe_mul(oracle_scale_mult.cast()?)?
			.safe_div(oracle_scale_div.cast()?)?
			.cast::<i64>()
	}

	pub fn update_volume_24h(
		&mut self,
		quote_asset_amount: u64,
		position_direction: PositionDirection,
		now: i64
	) -> NormalResult {
		let since_last = max(1_i64, now.safe_sub(self.last_trade_ts)?);

		amm::update_amm_long_short_intensity(
			self,
			now,
			quote_asset_amount,
			position_direction
		)?;

		self.volume_24h = stats::calculate_rolling_sum(
			self.volume_24h,
			quote_asset_amount,
			since_last,
			TWENTY_FOUR_HOUR
		)?;

		self.last_trade_ts = now;

		Ok(())
	}

	pub fn get_new_oracle_conf_pct(
		&self,
		confidence: u64, // price precision
		reserve_price: u64, // price precision
		now: i64
	) -> NormalResult<u64> {
		// use previous value decayed as lower bound to avoid shrinking too quickly
		let upper_bound_divisor = 21_u64;
		let lower_bound_divisor = 5_u64;
		let since_last = now
			.safe_sub(self.historical_oracle_data.last_oracle_price_twap_ts)?
			.max(0);

		let confidence_lower_bound = if since_last > 0 {
			let confidence_divisor = upper_bound_divisor
				.saturating_sub(since_last.cast::<u64>()?)
				.max(lower_bound_divisor);
			self.last_oracle_conf_pct.safe_sub(
				self.last_oracle_conf_pct / confidence_divisor
			)?
		} else {
			self.last_oracle_conf_pct
		};

		Ok(
			confidence
				.safe_mul(BID_ASK_SPREAD_PRECISION)?
				.safe_div(reserve_price)?
				.max(confidence_lower_bound)
		)
	}

	pub fn is_recent_oracle_valid(
		&self,
		current_slot: u64
	) -> NormalResult<bool> {
		Ok(self.last_oracle_valid && current_slot == self.last_update_slot)
	}
}

#[cfg(test)]
impl AMM {
	pub fn default_test() -> Self {
		let default_reserves = 100 * AMM_RESERVE_PRECISION;
		// make sure tests dont have the default sqrt_k = 0
		AMM {
			base_asset_reserve: default_reserves,
			quote_asset_reserve: default_reserves,
			sqrt_k: default_reserves,
			concentration_coef: MAX_CONCENTRATION_COEFFICIENT,
			order_step_size: 1,
			order_tick_size: 1,
			max_base_asset_reserve: u64::MAX as u128,
			min_base_asset_reserve: 0,
			terminal_quote_asset_reserve: default_reserves,
			peg_multiplier: crate::math::constants::PEG_PRECISION,
			max_fill_reserve_fraction: 1,
			max_spread: 1000,
			historical_oracle_data: HistoricalOracleData {
				last_oracle_price: PRICE_PRECISION_I64,
				..HistoricalOracleData::default()
			},
			last_oracle_valid: true,
			..AMM::default()
		}
	}

	pub fn default_btc_test() -> Self {
		AMM {
			base_asset_reserve: 65 * AMM_RESERVE_PRECISION,
			quote_asset_reserve: 63015384615,
			terminal_quote_asset_reserve: 64 * AMM_RESERVE_PRECISION,
			sqrt_k: 64 * AMM_RESERVE_PRECISION,

			peg_multiplier: 19_400_000_000,

			concentration_coef: MAX_CONCENTRATION_COEFFICIENT,
			max_base_asset_reserve: 90 * AMM_RESERVE_PRECISION,
			min_base_asset_reserve: 45 * AMM_RESERVE_PRECISION,

			base_asset_amount_with_amm: -(AMM_RESERVE_PRECISION as i128),
			mark_std: PRICE_PRECISION as u64,

			quote_asset_amount: 19_000_000_000, // short 1 BTC @ $19000
			historical_oracle_data: HistoricalOracleData {
				last_oracle_price: 19_400 * PRICE_PRECISION_I64,
				last_oracle_price_twap: 19_400 * PRICE_PRECISION_I64,
				last_oracle_price_twap_ts: 1662800000_i64,
				..HistoricalOracleData::default()
			},
			last_mark_price_twap_ts: 1662800000,

			curve_update_intensity: 100,

			base_spread: 250,
			max_spread: 975,
			last_oracle_valid: true,
			..AMM::default()
		}
	}
}
