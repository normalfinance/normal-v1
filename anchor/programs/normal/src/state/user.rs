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
use crate::math::synth_balance::{
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

use super::schedule::{Schedule};
use super::synth_market_map::SynthMarketMap;
use super::position::Position;

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
	//
	pub vault: Pubkey,
	/// Encoded display name e.g. "toly"
	pub name: [u8; 32],
	/// The user's positions
	pub positions: [Position; 8],
	/// The user's created indexes (market_index)
	pub indexes: [u16; 8],
	/// The user's Dollar-Cost Average (DCA) rules
	pub schedules: [Schedule; 8],
	pub schedule_streak: u16,
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
	// Status

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

	pub fn add_user_status(&mut self, status: UserStatus) {
		self.status |= status as u8;
	}

	pub fn remove_user_status(&mut self, status: UserStatus) {
		self.status &= !(status as u8);
	}

	// DCA

	pub fn get_dca(&self, market_index: u16) -> DriftResult<&PerpPosition> {
		Ok(
			&self.dollar_cost_averages
				[get_position_index(&self.dollar_cost_averages, market_index)?]
		)
	}

	pub fn get_dca_mut(
		&mut self,
		market_index: u16
	) -> DriftResult<&mut PerpPosition> {
		Ok(
			&mut self.dollar_cost_averages
				[get_position_index(&self.dollar_cost_averages, market_index)?]
		)
	}

	pub fn force_get_dca_mut(
		&mut self,
		market_index: u16
	) -> DriftResult<&mut PerpPosition> {
		let position_index = get_position_index(
			&self.dollar_cost_averages,
			market_index
		).or_else(|_|
			add_new_position(&mut self.dollar_cost_averages, market_index)
		)?;
		Ok(&mut self.dollar_cost_averages[position_index])
	}

	// Position

	pub fn get_position_index(&self, market_index: u16) -> NormalResult<usize> {
		// first spot position is always quote asset
		if market_index == 0 {
			validate!(
				self.positions[0].market_index == 0,
				ErrorCode::DefaultError,
				"User position 0 not market_index=0"
			)?;
			return Ok(0);
		}

		self.positions
			.iter()
			.position(|position| position.market_index == market_index)
			.ok_or(ErrorCode::CouldNotFindSpotPosition)
	}

	pub fn get_position(&self, market_index: u16) -> NormalResult<&SpotPosition> {
		self
			.get_position_index(market_index)
			.map(|market_index| &self.positions[market_index])
	}

	pub fn get_position_mut(
		&mut self,
		market_index: u16
	) -> NormalResult<&mut SpotPosition> {
		self
			.get_position_index(market_index)
			.map(move |market_index| &mut self.positions[market_index])
	}

	pub fn add_position(
		&mut self,
		market_index: u16,
		balance_type: SpotBalanceType
	) -> NormalResult<usize> {
		let new_position_index = self.positions
			.iter()
			.enumerate()
			.position(|(index, position)| index != 0 && position.is_available())
			.ok_or(ErrorCode::NoSpotPositionAvailable)?;

		let new_position = SpotPosition {
			market_index,
			balance_type,
			..SpotPosition::default()
		};

		self.positions[new_position_index] = new_position;

		Ok(new_position_index)
	}

	pub fn force_get_position_mut(
		&mut self,
		market_index: u16
	) -> NormalResult<&mut SpotPosition> {
		self
			.get_position_index(market_index)
			.or_else(|_| self.add_position(market_index, SpotBalanceType::Deposit))
			.map(move |market_index| &mut self.positions[market_index])
	}

	pub fn force_get_position_index(
		&mut self,
		market_index: u16
	) -> NormalResult<usize> {
		self
			.get_position_index(market_index)
			.or_else(|_| self.add_position(market_index, SpotBalanceType::Deposit))
	}

	// Index

	pub fn get_index(&self, market_index: u16) -> NormalResult<&SpotPosition> {
		self
			.get_index_index(market_index)
			.map(|market_index| &self.indexes[market_index])
	}

	// Deposit/Withdrawal

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
		oracle_map: &mut OracleMap,
		context: MarginContext,
		now: i64
	) -> NormalResult<MarginCalculation> {
		let margin_calculation =
			calculate_margin_requirement_and_total_collateral_and_liability_info(
				self,
				market_map,
				oracle_map,
				context
			)?;

		Ok(margin_calculation)
	}

	pub fn meets_withdraw_margin_requirement(
		&mut self,
		market_map: &MarketMap,
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
	/// Total index management fee paid
	/// precision: QUOTE_PRECISION
	pub total_expense_ratio_paid: u64,
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
