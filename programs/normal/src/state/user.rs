use crate::controller::lp::apply_lp_rebase_to_perp_position;
use crate::controller::position::{
	add_new_position,
	get_position_index,
	PositionDirection,
};
use crate::error::{ NormalResult, ErrorCode };
use crate::math::auction::{ calculate_auction_price, is_auction_complete };
use crate::math::casting::Cast;
use crate::math::constants::{
	EPOCH_DURATION,
	FUEL_START_TS,
	OPEN_ORDER_MARGIN_REQUIREMENT,
	PRICE_TIMES_AMM_TO_QUOTE_PRECISION_RATIO,
	QUOTE_PRECISION,
	QUOTE_SPOT_MARKET_INDEX,
	THIRTY_DAY,
};
use crate::math::lp::{
	calculate_lp_open_bids_asks,
	calculate_settle_lp_metrics,
};
use crate::math::margin::MarginRequirementType;
use crate::math::orders::{ standardize_base_asset_amount, standardize_price };
use crate::math::position::{
	calculate_base_asset_value_and_pnl_with_oracle_price,
	calculate_base_asset_value_with_oracle_price,
	calculate_perp_liability_value,
};
use crate::math::safe_math::SafeMath;
use crate::math::spot_balance::{
	get_signed_token_amount,
	get_strict_token_value,
	get_token_amount,
	get_token_value,
};
use crate::math::stats::calculate_rolling_sum;
use crate::state::oracle::StrictOraclePrice;
use crate::state::perp_market::{ ContractType, PerpMarket };
use crate::state::spot_market::{ SpotBalance, SpotBalanceType, SpotMarket };
use crate::state::traits::Size;
use crate::{ get_then_update_id, QUOTE_PRECISION_U64 };
use crate::{ math_error, SPOT_WEIGHT_PRECISION_I128 };
use crate::{ safe_increment, SPOT_WEIGHT_PRECISION };
use crate::{ validate, MAX_PREDICTION_MARKET_PRICE };
use anchor_lang::prelude::*;
use borsh::{ BorshDeserialize, BorshSerialize };
use solana_program::msg;
use std::cmp::max;
use std::fmt;
use std::ops::Neg;
use std::panic::Location;

use crate::math::margin::{
	calculate_margin_requirement_and_total_collateral_and_liability_info,
	validate_any_isolated_tier_requirements,
};
use crate::state::margin_calculation::{ MarginCalculation, MarginContext };
use crate::state::oracle_map::OracleMap;
use crate::state::perp_market_map::PerpMarketMap;
use crate::state::spot_market_map::SpotMarketMap;

use super::vault::Vault;

// #[cfg(test)]
// mod tests;

#[derive(Clone, Copy, BorshSerialize, BorshDeserialize, PartialEq, Debug, Eq)]
pub enum UserStatus {
	// Active = 0
	BeingLiquidated = 0b00000001,
	Bankrupt = 0b00000010,
	ReduceOnly = 0b00000100,
	AdvancedLp = 0b00001000,
}

// implement SIZE const for User
impl Size for User {
	const SIZE: usize = 4376;
}

#[account(zero_copy(unsafe))]
#[derive(Default, Eq, PartialEq, Debug)]
#[repr(C)]
pub struct User {
	/// The owner/authority of the account
	pub authority: Pubkey,
	/// An addresses that can control the account on the authority's behalf. Has limited power, cant withdraw
	pub delegate: Pubkey,
	/// Encoded display name e.g. "toly"
	pub name: [u8; 32],
	/// The user's vaults
	pub vaults: [Vault; 8],
	/// The total socialized loss the users has incurred upon the protocol
	/// precision: QUOTE_PRECISION
	pub total_social_loss: u64,
	/// Fees (taker fees, maker rebate, filler reward) for spot
	/// precision: QUOTE_PRECISION
	pub cumulative_spot_fees: i64,
	/// The amount of margin freed during liquidation. Used to force the liquidation to occur over a period of time
	/// Defaults to zero when not being liquidated
	/// precision: QUOTE_PRECISION
	pub liquidation_margin_freed: u64,
	/// The last slot a user was active. Used to determine if a user is idle
	pub last_active_slot: u64,
	/// Custom max initial margin ratio for the user
	pub max_margin_ratio: u32,
	/// The next liquidation id to be used for user
	pub next_liquidation_id: u16,
	/// The sub account id for this user
	pub sub_account_id: u16,
	/// Whether the user is active, being liquidated or bankrupt
	pub status: u8,
	/// User is idle if they haven't interacted with the protocol in 1 week and they have no orders, perp positions or borrows
	/// Off-chain keeper bots can ignore users that are idle
	pub idle: bool,
	pub padding1: [u8; 5],
	pub padding: [u8; 12],
}

impl User {
	pub fn is_being_liquidated(&self) -> bool {
		self.status &
			((UserStatus::BeingLiquidated as u8) | (UserStatus::Bankrupt as u8)) > 0
	}

	pub fn is_bankrupt(&self) -> bool {
		self.status & (UserStatus::Bankrupt as u8) > 0
	}

	pub fn is_reduce_only(&self) -> bool {
		self.status & (UserStatus::ReduceOnly as u8) > 0
	}

	pub fn is_advanced_lp(&self) -> bool {
		self.status & (UserStatus::AdvancedLp as u8) > 0
	}

	pub fn add_user_status(&mut self, status: UserStatus) {
		self.status |= status as u8;
	}

	pub fn remove_user_status(&mut self, status: UserStatus) {
		self.status &= !(status as u8);
	}

	pub fn increment_total_socialized_loss(&mut self, value: u64) -> NormalResult {
		self.total_social_loss = self.total_social_loss.saturating_add(value);

		Ok(())
	}

	pub fn update_cumulative_spot_fees(&mut self, amount: i64) -> NormalResult {
		safe_increment!(self.cumulative_spot_fees, amount);
		Ok(())
	}

	pub fn enter_liquidation(&mut self, slot: u64) -> NormalResult<u16> {
		if self.is_being_liquidated() {
			return self.next_liquidation_id.safe_sub(1);
		}

		self.add_user_status(UserStatus::BeingLiquidated);
		self.liquidation_margin_freed = 0;
		self.last_active_slot = slot;
		Ok(get_then_update_id!(self, next_liquidation_id))
	}

	pub fn exit_liquidation(&mut self) {
		self.remove_user_status(UserStatus::BeingLiquidated);
		self.remove_user_status(UserStatus::Bankrupt);
		self.liquidation_margin_freed = 0;
	}

	pub fn enter_bankruptcy(&mut self) {
		self.remove_user_status(UserStatus::BeingLiquidated);
		self.add_user_status(UserStatus::Bankrupt);
	}

	pub fn exit_bankruptcy(&mut self) {
		self.remove_user_status(UserStatus::BeingLiquidated);
		self.remove_user_status(UserStatus::Bankrupt);
		self.liquidation_margin_freed = 0;
	}

	pub fn increment_margin_freed(&mut self, margin_free: u64) -> NormalResult {
		self.liquidation_margin_freed =
			self.liquidation_margin_freed.safe_add(margin_free)?;
		Ok(())
	}

	pub fn update_last_active_slot(&mut self, slot: u64) {
		if !self.is_being_liquidated() {
			self.last_active_slot = slot;
		}
		self.idle = false;
	}

	pub fn update_reduce_only_status(
		&mut self,
		reduce_only: bool
	) -> NormalResult {
		if reduce_only {
			self.add_user_status(UserStatus::ReduceOnly);
		} else {
			self.remove_user_status(UserStatus::ReduceOnly);
		}

		Ok(())
	}

	pub fn update_advanced_lp_status(
		&mut self,
		advanced_lp: bool
	) -> NormalResult {
		if advanced_lp {
			self.add_user_status(UserStatus::AdvancedLp);
		} else {
			self.remove_user_status(UserStatus::AdvancedLp);
		}

		Ok(())
	}

	pub fn calculate_margin(
		&mut self,
		perp_market_map: &PerpMarketMap,
		spot_market_map: &SpotMarketMap,
		oracle_map: &mut OracleMap,
		context: MarginContext,
		now: i64
	) -> NormalResult<MarginCalculation> {
		let margin_calculation =
			calculate_margin_requirement_and_total_collateral_and_liability_info(
				self,
				perp_market_map,
				spot_market_map,
				oracle_map,
				context
			)?;

		Ok(margin_calculation)
	}
}

#[zero_copy(unsafe)]
#[derive(Default, Eq, PartialEq, Debug)]
#[repr(C)]
pub struct UserFees {
	/// Total taker fee paid
	/// precision: QUOTE_PRECISION
	pub total_fee_paid: u64,
	/// Total maker fee rebate
	/// precision: QUOTE_PRECISION
	pub total_fee_rebate: u64,
	/// Total discount from holding token
	/// precision: QUOTE_PRECISION
	pub total_token_discount: u64,
	/// Total discount from being referred
	/// precision: QUOTE_PRECISION
	pub total_referee_discount: u64,
	/// Total reward to referrer
	/// precision: QUOTE_PRECISION
	pub total_referrer_reward: u64,
	/// Total reward to referrer this epoch
	/// precision: QUOTE_PRECISION
	pub current_epoch_referrer_reward: u64,
}

#[derive(
	Default,
	Clone,
	Copy,
	BorshSerialize,
	BorshDeserialize,
	PartialEq,
	Debug,
	Eq
)]
pub enum MarketType {
	#[default]
	Synth,
}

impl fmt::Display for MarketType {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			MarketType::Synth => write!(f, "Synth"),
		}
	}
}

#[account(zero_copy(unsafe))]
#[derive(Eq, PartialEq, Debug)]
#[repr(C)]
#[derive(Default)]
pub struct UserStats {
	/// The authority for all of a users sub accounts
	pub authority: Pubkey,
	/// The address that referred this user
	pub referrer: Pubkey,
	/// Stats on the fees paid by the user
	pub fees: UserFees,

	/// The timestamp of the next epoch
	/// Epoch is used to limit referrer rewards earned in single epoch
	pub next_epoch_ts: i64,

	/// The amount of tokens staked in the quote spot markets if
	pub if_staked_quote_asset_amount: u64,
	/// The current number of sub accounts
	pub number_of_sub_accounts: u16,
	/// The number of sub accounts created. Can be greater than the number of sub accounts if user
	/// has deleted sub accounts
	pub number_of_sub_accounts_created: u16,
	/// Whether the user is a referrer. Sub account 0 can not be deleted if user is a referrer
	pub is_referrer: bool,
	pub disable_update_perp_bid_ask_twap: bool,
	pub padding1: [u8; 2],

	/// The amount of tokens staked in the governance spot markets if
	pub if_staked_gov_token_amount: u64,

	pub padding: [u8; 12],
}

impl Size for UserStats {
	const SIZE: usize = 240;
}

impl UserStats {
	pub fn increment_total_fees(&mut self, fee: u64) -> NormalResult {
		self.fees.total_fee_paid = self.fees.total_fee_paid.safe_add(fee)?;

		Ok(())
	}

	pub fn increment_total_rebate(&mut self, fee: u64) -> NormalResult {
		self.fees.total_fee_rebate = self.fees.total_fee_rebate.safe_add(fee)?;

		Ok(())
	}

	pub fn increment_total_referrer_reward(
		&mut self,
		reward: u64,
		now: i64
	) -> NormalResult {
		self.fees.total_referrer_reward =
			self.fees.total_referrer_reward.safe_add(reward)?;

		self.fees.current_epoch_referrer_reward =
			self.fees.current_epoch_referrer_reward.safe_add(reward)?;

		if now > self.next_epoch_ts {
			let n_epoch_durations = now
				.safe_sub(self.next_epoch_ts)?
				.safe_div(EPOCH_DURATION)?
				.safe_add(1)?;

			self.next_epoch_ts = self.next_epoch_ts.safe_add(
				EPOCH_DURATION.safe_mul(n_epoch_durations)?
			)?;

			self.fees.current_epoch_referrer_reward = 0;
		}

		Ok(())
	}

	pub fn increment_total_referee_discount(
		&mut self,
		discount: u64
	) -> NormalResult {
		self.fees.total_referee_discount =
			self.fees.total_referee_discount.safe_add(discount)?;

		Ok(())
	}

	pub fn has_referrer(&self) -> bool {
		!self.referrer.eq(&Pubkey::default())
	}

	pub fn get_total_30d_volume(&self) -> NormalResult<u64> {
		self.taker_volume_30d.safe_add(self.maker_volume_30d)
	}

	pub fn get_age_ts(&self, now: i64) -> i64 {
		// upper bound of age of the user stats account
		let min_action_ts: i64 = self.last_filler_volume_30d_ts
			.min(self.last_maker_volume_30d_ts)
			.min(self.last_taker_volume_30d_ts);
		now.saturating_sub(min_action_ts).max(0)
	}
}

#[account(zero_copy(unsafe))]
#[derive(Default, Eq, PartialEq, Debug)]
#[repr(C)]
pub struct ReferrerName {
	pub authority: Pubkey,
	pub user: Pubkey,
	pub user_stats: Pubkey,
	pub name: [u8; 32],
}

impl Size for ReferrerName {
	const SIZE: usize = 136;
}
