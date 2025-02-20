use crate::errors::{ NormalResult, ErrorCode };
use crate::math::casting::Cast;

use crate::math::constants::QUOTE_PRECISION_U64;
use crate::math::safe_math::SafeMath;
use crate::state::traits::Size;
use crate::{ get_then_update_id, safe_increment, validate };
use anchor_lang::prelude::*;
use borsh::{ BorshDeserialize, BorshSerialize };
use solana_program::msg;
use crate::math_error;

use super::user_stats::UserStats;

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
	/// The user's positions
	// pub positions: [CollateralPosition; 8],
	/// The total values of deposits the user has made
	/// precision: QUOTE_PRECISION
	pub total_deposits: u64,
	/// The total values of withdrawals the user has made
	/// precision: QUOTE_PRECISION
	pub total_withdraws: u64,
	/// The total socialized loss the users has incurred upon the protocol
	/// precision: QUOTE_PRECISION
	pub total_social_loss: u64,
	/// Fees (taker fees, maker rebate, filler reward) for spot
	/// precision: QUOTE_PRECISION
	pub cumulative_swap_fees: i64,
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

	pub fn add_user_status(&mut self, status: UserStatus) {
		self.status |= status as u8;
	}

	pub fn remove_user_status(&mut self, status: UserStatus) {
		self.status &= !(status as u8);
	}

	// Position...

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
		let value = self.get_deposit_value(amount, price, precision)?;
		self.total_deposits = self.total_deposits.saturating_add(value);

		Ok(())
	}

	pub fn increment_total_withdraws(
		&mut self,
		amount: u64,
		price: i64,
		precision: u128
	) -> NormalResult {
		let value = self.get_deposit_value(amount, price, precision)?;
		self.total_withdraws = self.total_withdraws.saturating_add(value);

		Ok(())
	}

	pub fn increment_total_socialized_loss(
		&mut self,
		value: u64
	) -> NormalResult {
		self.total_social_loss = self.total_social_loss.saturating_add(value);

		Ok(())
	}

	pub fn update_cumulative_swap_fees(&mut self, amount: i64) -> NormalResult {
		safe_increment!(self.cumulative_swap_fees, amount);
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

	pub fn qualifies_for_withdraw_fee(
		&self,
		user_stats: &UserStats,
		slot: u64
	) -> bool {
		// only qualifies for user with recent last_active_slot (~25 seconds)
		if slot.saturating_sub(self.last_active_slot) >= 50 {
			return false;
		}

		let min_total_withdraws = 10_000_000 * QUOTE_PRECISION_U64; // $10M

		// if total withdraws are greater than $10M and user has paid more than %.01 of it in fees
		self.total_withdraws >= min_total_withdraws &&
			self.total_withdraws / user_stats.fees.total_fee_paid.max(1) > 10_000
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

	// pub fn calculate_margin(
	// 	&mut self,
	// 	market_map: &MarketMap,
	// 	oracle_map: &mut OracleMap,
	// 	context: MarginContext,
	// 	now: i64
	// ) -> NormalResult<MarginCalculation> {
	// 	let margin_calculation =
	// 		calculate_margin_requirement_and_total_collateral_and_liability_info(
	// 			self,
	// 			market_map,
	// 			oracle_map,
	// 			context
	// 		)?;

	// 	Ok(margin_calculation)
	// }

	// pub fn meets_withdraw_margin_requirement(
	// 	&mut self,
	// 	market_map: &MarketMap,
	// 	oracle_map: &mut OracleMap,
	// 	margin_requirement_type: MarginRequirementType,
	// 	withdraw_market_index: u16,
	// 	withdraw_amount: u128,
	// 	user_stats: &mut UserStats,
	// 	now: i64
	// ) -> NormalResult<bool> {
	// 	let strict = margin_requirement_type == MarginRequirementType::Initial;
	// 	let context = MarginContext::standard(margin_requirement_type).strict(
	// 		strict
	// 	);

	// 	let calculation =
	// 		calculate_margin_requirement_and_total_collateral_and_liability_info(
	// 			self,
	// 			market_map,
	// 			oracle_map,
	// 			context
	// 		)?;

	// 	if
	// 		calculation.margin_requirement > 0 ||
	// 		calculation.get_num_of_liabilities()? > 0
	// 	{
	// 		validate!(
	// 			calculation.all_oracles_valid,
	// 			ErrorCode::InvalidOracle,
	// 			"User attempting to withdraw with outstanding liabilities when an oracle is invalid"
	// 		)?;
	// 	}

	// 	validate_any_isolated_tier_requirements(self, calculation)?;

	// 	validate!(
	// 		calculation.meets_margin_requirement(),
	// 		ErrorCode::InsufficientCollateral,
	// 		"User attempting to withdraw where total_collateral {} is below initial_margin_requirement {}",
	// 		calculation.total_collateral,
	// 		calculation.margin_requirement
	// 	)?;

	// 	Ok(true)
	// }
}
