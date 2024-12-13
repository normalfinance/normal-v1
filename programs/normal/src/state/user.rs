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
use crate::math::margin::MarginRequirementType;
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

use super::market_map::MarketMap;
use super::vault::Vault;
use super::vault_map::VaultMap;
use super::vp::VaultPosition;

// #[cfg(test)]
// mod tests;

#[derive(Clone, Copy, BorshSerialize, BorshDeserialize, PartialEq, Debug, Eq)]
pub enum UserStatus {
	// Active = 0
	BeingLiquidated = 0b00000001,
	Bankrupt = 0b00000010,
	ReduceOnly = 0b00000100,
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
	/// The user's vault positions
	pub vault_positions: [VaultPosition; 8],
	/// The total values of deposits the user has made
	/// precision: QUOTE_PRECISION
	pub total_deposits: u64,
	/// The total values of withdrawals the user has made
	/// precision: QUOTE_PRECISION
	pub total_withdraws: u64,
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

	pub fn get_vault_position_index(
		&self,
		vault_index: u16
	) -> NormalResult<usize> {
		// first spot position is always quote asset
		if vault_index == 0 {
			validate!(
				self.vault_positions[0].vault_index == 0,
				ErrorCode::DefaultError,
				"User position 0 not vault_index=0"
			)?;
			return Ok(0);
		}

		self.vault_positions
			.iter()
			.position(|vault_position| vault_position.vault_index == vault_index)
			.ok_or(ErrorCode::CouldNotFindSpotPosition)
	}

	pub fn get_vault_position(
		&self,
		vault_index: u16
	) -> NormalResult<&SpotPosition> {
		self
			.get_vault_position_index(vault_index)
			.map(|vault_index| &self.vault_positions[vault_index])
	}

	pub fn get_vault_position_mut(
		&mut self,
		vault_index: u16
	) -> NormalResult<&mut SpotPosition> {
		self
			.get_vault_position_index(vault_index)
			.map(move |vault_index| &mut self.vault_positions[vault_index])
	}

	pub fn get_quote_vault_position(&self) -> &SpotPosition {
		match self.get_vault_position(QUOTE_SPOT_MARKET_INDEX) {
			Ok(position) => position,
			Err(_) => unreachable!(),
		}
	}

	pub fn get_quote_vault_position_mut(&mut self) -> &mut SpotPosition {
		match self.get_vault_position_mut(QUOTE_SPOT_MARKET_INDEX) {
			Ok(position) => position,
			Err(_) => unreachable!(),
		}
	}

	pub fn add_vault_position(
		&mut self,
		market_index: u16,
		balance_type: SpotBalanceType
	) -> NormalResult<usize> {
		let new_vault_position_index = self.vault_positions
			.iter()
			.enumerate()
			.position(
				|(index, vault_position)| index != 0 && vault_position.is_available()
			)
			.ok_or(ErrorCode::NoSpotPositionAvailable)?;

		let new_vault_position = SpotPosition {
			market_index,
			balance_type,
			..SpotPosition::default()
		};

		self.vault_positions[new_vault_position_index] = new_vault_position;

		Ok(new_vault_position_index)
	}

	pub fn force_get_vault_position_mut(
		&mut self,
		vault_index: u16
	) -> NormalResult<&mut SpotPosition> {
		self
			.get_vault_position_index(vault_index)
			.or_else(|_|
				self.add_vault_position(vault_index, SpotBalanceType::Deposit)
			)
			.map(move |vault_index| &mut self.vault_positions[vault_index])
	}

	pub fn force_get_vault_position_index(
		&mut self,
		vault_index: u16
	) -> NormalResult<usize> {
		self
			.get_vault_position_index(vault_index)
			.or_else(|_|
				self.add_vault_position(vault_index, SpotBalanceType::Deposit)
			)
	}

	pub fn get_deposit_value(
		&mut self,
		amount: u64,
		price: i64,
		precision: u128
	) -> NormalResult<u64> {
		let value = amount
			.cast::<u128>()?
			.safe_mul(price.cast::<u128>()?)?
			.safe_div(precision)?
			.cast::<u64>()?;

		Ok(value)
	}

	pub fn increment_total_deposits(
		&mut self,
		amount: u64,
		price: i64,
		precision: u128
	) -> NormalResult {
		let value = self.get_deposit_value(amount, price, precision);
		self.total_deposits = self.total_deposits.saturating_add(value);

		Ok(())
	}

	pub fn increment_total_withdraws(
		&mut self,
		amount: u64,
		price: i64,
		precision: u128
	) -> NormalResult {
		let value = amount
			.cast::<u128>()?
			.safe_mul(price.cast()?)?
			.safe_div(precision)?
			.cast::<u64>()?;
		self.total_withdraws = self.total_withdraws.saturating_add(value);

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

	pub fn calculate_margin(
		&mut self,
		market_map: &MarketMap,
		vault_map: &VaultMap,
		oracle_map: &mut OracleMap,
		context: MarginContext,
		now: i64
	) -> NormalResult<MarginCalculation> {
		let margin_calculation =
			calculate_margin_requirement_and_total_collateral_and_liability_info(
				self,
				market_map,
				vault_map,
				oracle_map,
				context
			)?;

		Ok(margin_calculation)
	}

	pub fn meets_withdraw_margin_requirement(
		&mut self,
		market_map: &MarketMap,
		vault_map: &VaultMap,
		oracle_map: &mut OracleMap,
		margin_requirement_type: MarginRequirementType,
		withdraw_market_index: u16,
		withdraw_amount: u128,
		user_stats: &mut UserStats,
		now: i64
	) -> NormalResult<bool> {
		let strict = margin_requirement_type == MarginRequirementType::Initial;
		let context = MarginContext::standard(margin_requirement_type).strict(
			strict
		);

		let calculation =
			calculate_margin_requirement_and_total_collateral_and_liability_info(
				self,
				market_map,
				vault_map,
				oracle_map,
				context
			)?;

		if
			calculation.margin_requirement > 0 ||
			calculation.get_num_of_liabilities()? > 0
		{
			validate!(
				calculation.all_oracles_valid,
				ErrorCode::InvalidOracle,
				"User attempting to withdraw with outstanding liabilities when an oracle is invalid"
			)?;
		}

		validate_any_isolated_tier_requirements(self, calculation)?;

		validate!(
			calculation.meets_margin_requirement(),
			ErrorCode::InsufficientCollateral,
			"User attempting to withdraw where total_collateral {} is below initial_margin_requirement {}",
			calculation.total_collateral,
			calculation.margin_requirement
		)?;

		Ok(true)
	}
}

#[zero_copy(unsafe)]
#[derive(Default, Eq, PartialEq, Debug)]
#[repr(C)]
pub struct UserFees {
	/// Total taker fee paid
	/// precision: QUOTE_PRECISION
	pub total_fee_paid: u64,
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

	/// Rolling 30day collateral volume for user
	/// precision: QUOTE_PRECISION
	pub collateral_volume_30d: u64,
	/// Rolling 30day trade volume for user
	/// precision: QUOTE_PRECISION
	pub trade_volume_30d: u64,
	/// last time the collateral volume was updated
	pub last_collateral_volume_30d_ts: i64,
	/// last time the trade volume was updated
	pub last_trade_volume_30d_ts: i64,

	/// The amount of tokens staked in the quote spot markets if
	pub if_staked_quote_asset_amount: u64,
	/// The current number of sub accounts
	pub number_of_sub_accounts: u16,
	/// The number of sub accounts created. Can be greater than the number of sub accounts if user
	/// has deleted sub accounts
	pub number_of_sub_accounts_created: u16,
	/// Whether the user is a referrer. Sub account 0 can not be deleted if user is a referrer
	pub is_referrer: bool,
	pub padding: [u8; 12],
}

impl Size for UserStats {
	const SIZE: usize = 168;
}

impl UserStats {
	pub fn update_collateral_volume_30d(
		&mut self,
		quote_asset_amount: u64,
		now: i64
	) -> NormalResult {
		let since_last = max(
			1_i64,
			now.safe_sub(self.last_collateral_volume_30d_ts)?
		);

		self.collateral_volume_30d = calculate_rolling_sum(
			self.collateral_volume_30d,
			quote_asset_amount,
			since_last,
			THIRTY_DAY
		)?;
		self.last_collateral_volume_30d_ts = now;

		Ok(())
	}

	pub fn update_trade_volume_30d(
		&mut self,
		quote_asset_amount: u64,
		now: i64
	) -> NormalResult {
		let since_last = max(1_i64, now.safe_sub(self.last_trade_volume_30d_ts)?);

		self.trade_volume_30d = calculate_rolling_sum(
			self.trade_volume_30d,
			quote_asset_amount,
			since_last,
			THIRTY_DAY
		)?;
		self.last_trade_volume_30d_ts = now;

		Ok(())
	}

	pub fn increment_total_fees(&mut self, fee: u64) -> NormalResult {
		self.fees.total_fee_paid = self.fees.total_fee_paid.safe_add(fee)?;

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
		self.trade_volume_30d.safe_add(self.collateral_volume_30d)
	}

	pub fn get_age_ts(&self, now: i64) -> i64 {
		// upper bound of age of the user stats account
		let min_action_ts: i64 = self.last_collateral_volume_30d_ts.min(
			self.last_trade_volume_30d_ts
		);
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
