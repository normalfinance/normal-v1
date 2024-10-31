use crate::controller::position::{
	add_new_position,
	get_position_index,
	OrderSide,
};
use crate::error::{ NormalResult, ErrorCode };
use crate::math::auction::{ calculate_auction_price, is_auction_complete };
use crate::math::casting::Cast;
use crate::constants::constants::{
	EPOCH_DURATION,
	PRICE_TIMES_AMM_TO_QUOTE_PRECISION_RATIO,
	QUOTE_PRECISION,
	QUOTE_SPOT_MARKET_INDEX,
	THIRTY_DAY,
};
use crate::math::orders::{ standardize_base_asset_amount, standardize_price };
use crate::math::position::{ calculate_base_asset_value_with_oracle_price };
use crate::math::safe_math::SafeMath;
use crate::math::balance::{
	get_signed_token_amount,
	get_strict_token_value,
	get_token_amount,
	get_token_value,
};
use crate::math::stats::calculate_rolling_sum;
use crate::state::oracle::StrictOraclePrice;
use crate::state::market::Market;
use crate::state::traits::Size;
use crate::{ get_then_update_id, QUOTE_PRECISION_U64 };
use crate::{ math_error, SPOT_WEIGHT_PRECISION_I128 };
use crate::{ safe_increment, SPOT_WEIGHT_PRECISION };
use crate::validate;
use anchor_lang::prelude::*;
use borsh::{ BorshDeserialize, BorshSerialize };
use solana_program::msg;
use std::cmp::max;
use std::fmt;
use std::ops::Neg;
use std::panic::Location;

use anchor_spl::token::{ mint_to, Mint, MintTo, Token, TokenAccount };

use crate::state::oracle_map::OracleMap;
use crate::state::market_map::MarketMap;

// #[cfg(test)]
// mod tests;

#[derive(Clone, Copy, BorshSerialize, BorshDeserialize, PartialEq, Debug, Eq)]
pub enum UserStatus {
	Active = 0,
	ReduceOnly = 0b00000001,
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
	pub positions: [Position; 8],
	/// The user's orders
	pub orders: [Order; 32],
	/// The last time the user added perp lp positions
	pub last_add_lp_shares_ts: i64,
	/// The last slot a user was active. Used to determine if a user is idle
	pub last_active_slot: u64,
	/// Every user order has an order id. This is the next order id to be used
	pub next_order_id: u32,
	/// The sub account id for this user
	pub sub_account_id: u16,
	/// Whether the user is active, being liquidated or bankrupt
	pub status: u8,
	/// User is idle if they haven't interacted with the protocol in 1 week and they have no orders, perp positions or borrows
	/// Off-chain keeper bots can ignore users that are idle
	pub idle: bool,
	/// number of open orders
	pub open_orders: u8,
	/// Whether or not user has open order
	pub has_open_order: bool,
	/// number of open orders with auction
	pub open_auctions: u8,
	/// Whether or not user has open order with auction
	pub has_open_auction: bool,
	pub padding1: [u8; 5],
	pub padding: [u8; 12],
}

impl User {
	pub fn is_reduce_only(&self) -> bool {
		self.status & (UserStatus::ReduceOnly as u8) > 0
	}

	pub fn add_user_status(&mut self, status: UserStatus) {
		self.status |= status as u8;
	}

	pub fn remove_user_status(&mut self, status: UserStatus) {
		self.status &= !(status as u8);
	}

	pub fn get_position(&self, market_index: u16) -> NormalResult<Position> {
		Ok(&self.positions[get_position_index(&self.positions, market_index)?])
	}

	pub fn get_position_mut(
		&mut self,
		market_index: u16
	) -> NormalResult<&mut Position> {
		Ok(&mut self.positions[get_position_index(&self.positions, market_index)?])
	}

	pub fn force_get_position_mut(
		&mut self,
		market_index: u16
	) -> NormalResult<&mut Position> {
		let position_index = get_position_index(
			&self.positions,
			market_index
		).or_else(|_| add_new_position(&mut self.positions, market_index))?;
		Ok(&mut self.positions[position_index])
	}

	pub fn get_order_index(&self, order_id: u32) -> NormalResult<usize> {
		self.orders
			.iter()
			.position(
				|order| order.order_id == order_id && order.status == OrderStatus::Open
			)
			.ok_or(ErrorCode::OrderDoesNotExist)
	}

	pub fn get_order_index_by_user_order_id(
		&self,
		user_order_id: u8
	) -> NormalResult<usize> {
		self.orders
			.iter()
			.position(|order| {
				order.user_order_id == user_order_id &&
					order.status == OrderStatus::Open
			})
			.ok_or(ErrorCode::OrderDoesNotExist)
	}

	pub fn get_order(&self, order_id: u32) -> Option<&Order> {
		self.orders.iter().find(|order| order.order_id == order_id)
	}

	pub fn get_last_order_id(&self) -> u32 {
		if self.next_order_id == 1 { u32::MAX } else { self.next_order_id - 1 }
	}

	pub fn update_last_active_slot(&mut self, slot: u64) {
		self.last_active_slot = slot;

		self.idle = false;
	}

	pub fn increment_open_orders(&mut self, is_auction: bool) {
		self.open_orders = self.open_orders.saturating_add(1);
		self.has_open_order = self.open_orders > 0;
		if is_auction {
			self.increment_open_auctions();
		}
	}

	pub fn increment_open_auctions(&mut self) {
		self.open_auctions = self.open_auctions.saturating_add(1);
		self.has_open_auction = self.open_auctions > 0;
	}

	pub fn decrement_open_orders(&mut self, is_auction: bool) {
		self.open_orders = self.open_orders.saturating_sub(1);
		self.has_open_order = self.open_orders > 0;
		if is_auction {
			self.open_auctions = self.open_auctions.saturating_sub(1);
			self.has_open_auction = self.open_auctions > 0;
		}
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

	pub fn has_room_for_new_order(&self) -> bool {
		for order in self.orders.iter() {
			if order.status == OrderStatus::Init {
				return true;
			}
		}

		false
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

#[zero_copy(unsafe)]
#[derive(Default, Eq, PartialEq, Debug)]
#[repr(C)]
pub struct Position {
	/// The amount of open bids the user has in this market
	/// precision: BASE_PRECISION
	pub open_bids: i64,
	/// The amount of open asks the user has in this market
	/// precision: BASE_PRECISION
	pub open_asks: i64,
	/// Settling LP position can lead to a small amount of base asset being left over smaller than step size
	/// This records that remainder so it can be settled later on
	/// precision: BASE_PRECISION
	pub remainder_base_asset_amount: i32,
	/// The market index for the market
	pub market_index: u16,
	/// The number of open orders
	pub open_orders: u8,

	pub padding: [u8; 4],
}

impl Position {
	pub fn is_for(&self, market_index: u16) -> bool {
		self.market_index == market_index && !self.is_available()
	}

	pub fn is_available(&self) -> bool {
		!self.is_open_position() && !self.has_open_order() && !self.is_lp()
	}

	pub fn base_asset_amount(&self, market_map: MarketMap) -> u64 {
		
		self.token_account.amount
	}

	pub fn is_open_position(&self) -> bool {
		self.base_asset_amount() != 0
	}

	pub fn has_open_order(&self) -> bool {
		self.open_orders != 0 || self.open_bids != 0 || self.open_asks != 0
	}

	/// The number of lp (liquidity provider) shares the user has in this market
	/// LP shares allow users to provide liquidity via the AMM
	/// precision: BASE_PRECISION
	pub fn lp_shares(&self) -> u64 {
		0
	}

	pub fn is_lp(&self) -> bool {
		self.lp_shares > 0
	}

	pub fn worst_case_base_asset_amount(
		&self,
		oracle_price: i64
	) -> NormalResult<i128> {
		self.worst_case_liability_value(oracle_price).map(|v| v.0)
	}

	pub fn worst_case_liability_value(
		&self,
		oracle_price: i64
	) -> NormalResult<(i128, u128)> {
		let base_asset_amount_all_bids_fill = self
			.base_asset_amount()
			.safe_add(self.open_bids)?
			.cast::<i128>()?;
		let base_asset_amount_all_asks_fill = self
			.base_asset_amount()
			.safe_add(self.open_asks)?
			.cast::<i128>()?;

		let liability_value_all_bids_fill =
			calculate_base_asset_value_with_oracle_price(
				base_asset_amount_all_bids_fill,
				oracle_price
			)?;

		let liability_value_all_asks_fill =
			calculate_base_asset_value_with_oracle_price(
				base_asset_amount_all_asks_fill,
				oracle_price
			)?;

		if liability_value_all_asks_fill >= liability_value_all_bids_fill {
			Ok((base_asset_amount_all_asks_fill, liability_value_all_asks_fill))
		} else {
			Ok((base_asset_amount_all_bids_fill, liability_value_all_bids_fill))
		}
	}

	pub fn get_side(&self) -> OrderSide {
		if self.base_asset_amount() >= 0 { OrderSide::Buy } else { OrderSide::Sell }
	}

	pub fn get_side_to_close(&self) -> OrderSide {
		if self.base_asset_amount() >= 0 { OrderSide::Sell } else { OrderSide::Buy }
	}

	pub fn get_base_asset_amount_with_remainder(&self) -> NormalResult<i128> {
		if self.remainder_base_asset_amount != 0 {
			self
				.base_asset_amount()
				.cast::<i128>()?
				.safe_add(self.remainder_base_asset_amount.cast::<i128>()?)
		} else {
			self.base_asset_amount().cast::<i128>()
		}
	}

	pub fn get_base_asset_amount_with_remainder_abs(&self) -> NormalResult<i128> {
		Ok(self.get_base_asset_amount_with_remainder()?.abs())
	}
}

pub(crate) type Positions = [Position; 8];

#[cfg(test)]
use crate::constants::constants::{
	AMM_TO_QUOTE_PRECISION_RATIO_I128,
	PRICE_PRECISION_I128,
};

#[derive(Clone, Copy, Default, Eq, PartialEq, Debug)]
pub struct OrderFillSimulation {
	pub token_amount: i128,
	pub orders_value: i128,
	pub token_value: i128,
	pub weighted_token_value: i128,
	pub free_collateral_contribution: i128,
}

impl OrderFillSimulation {
	pub fn riskier_side(ask: Self, bid: Self) -> Self {
		if ask.free_collateral_contribution <= bid.free_collateral_contribution {
			ask
		} else {
			bid
		}
	}

	pub fn risk_increasing(&self, after: Self) -> bool {
		after.free_collateral_contribution < self.free_collateral_contribution
	}
}

impl Position {
	pub fn is_available(&self) -> bool {
		self.scaled_balance == 0 && self.open_orders == 0
	}

	pub fn has_open_order(&self) -> bool {
		self.open_orders != 0 || self.open_bids != 0 || self.open_asks != 0
	}

	pub fn get_token_amount(&self, market: &Market) -> NormalResult<u128> {
		get_token_amount(self.scaled_balance.cast()?, market)
	}

	pub fn get_signed_token_amount(&self, market: &Market) -> NormalResult<i128> {
		get_signed_token_amount(
			get_token_amount(self.scaled_balance.cast()?, market)?
		)
	}

	pub fn get_worst_case_fill_simulation(
		&self,
		market: &Market,
		strict_oracle_price: &StrictOraclePrice,
		token_amount: Option<i128>
	) -> NormalResult<OrderFillSimulation> {
		let [bid_simulation, ask_simulation] = self.simulate_fills_both_sides(
			market,
			strict_oracle_price,
			token_amount
		)?;

		Ok(OrderFillSimulation::riskier_side(ask_simulation, bid_simulation))
	}

	pub fn simulate_fills_both_sides(
		&self,
		market: &Market,
		strict_oracle_price: &StrictOraclePrice,
		token_amount: Option<i128>
	) -> NormalResult<[OrderFillSimulation; 2]> {
		let token_amount = match token_amount {
			Some(token_amount) => token_amount,
			None => self.get_signed_token_amount(market)?,
		};

		let token_value = get_strict_token_value(
			token_amount,
			market.decimals,
			strict_oracle_price
		)?;

		let calculate_weighted_token_value = |
			token_amount: i128,
			token_value: i128
		| {
			token_value
		};

		if self.open_bids == 0 && self.open_asks == 0 {
			let weighted_token_value = calculate_weighted_token_value(
				token_amount,
				token_value
			)?;

			let calculation = OrderFillSimulation {
				token_amount,
				orders_value: 0,
				token_value,
				weighted_token_value,
				free_collateral_contribution: weighted_token_value,
			};

			return Ok([calculation, calculation]);
		}

		let simulate_side = |
			strict_oracle_price: &StrictOraclePrice,
			token_amount: i128,
			open_orders: i128
		| {
			let order_value = get_token_value(
				-open_orders,
				market.decimals,
				strict_oracle_price.max()
			)?;
			let token_amount_after_fill = token_amount.safe_add(open_orders)?;
			let token_value_after_fill = token_value.safe_add(order_value.neg())?;

			let weighted_token_value_after_fill = calculate_weighted_token_value(
				token_amount_after_fill,
				token_value_after_fill
			)?;

			let free_collateral_contribution =
				weighted_token_value_after_fill.safe_add(order_value)?;

			Ok(OrderFillSimulation {
				token_amount: token_amount_after_fill,
				orders_value: order_value,
				token_value: token_value_after_fill,
				weighted_token_value: weighted_token_value_after_fill,
				free_collateral_contribution,
			})
		};

		let bid_simulation = simulate_side(
			strict_oracle_price,
			token_amount,
			self.open_bids.cast()?
		)?;

		let ask_simulation = simulate_side(
			strict_oracle_price,
			token_amount,
			self.open_asks.cast()?
		)?;

		Ok([bid_simulation, ask_simulation])
	}
}

#[zero_copy(unsafe)]
#[repr(C)]
#[derive(AnchorSerialize, AnchorDeserialize, PartialEq, Debug, Eq)]
pub struct Order {
	/// The slot the order was placed
	pub slot: u64,
	/// The limit price for the order (can be 0 for market orders)
	/// For orders with an auction, this price isn't used until the auction is complete
	/// precision: PRICE_PRECISION
	pub price: u64,
	/// The size of the order
	/// precision for perps: BASE_PRECISION
	/// precision for spot: token mint precision
	pub base_asset_amount: u64,
	/// The amount of the order filled
	/// precision for perps: BASE_PRECISION
	/// precision for spot: token mint precision
	pub base_asset_amount_filled: u64,
	/// The amount of quote filled for the order
	/// precision: QUOTE_PRECISION
	pub quote_asset_amount_filled: u64,
	/// At what price the order will be triggered. Only relevant for trigger orders
	/// precision: PRICE_PRECISION
	pub trigger_price: u64,
	/// The start price for the auction. Only relevant for market orders
	/// precision: PRICE_PRECISION
	pub auction_start_price: i64,
	/// The end price for the auction. Only relevant for market orders
	/// precision: PRICE_PRECISION
	pub auction_end_price: i64,
	/// The time when the order will expire
	pub max_ts: i64,

	/// The id for the order. Each users has their own order id space
	pub order_id: u32,
	/// The perp/spot market index
	pub market_index: u16,
	/// Whether the order is open or unused
	pub status: OrderStatus,
	/// The type of order
	pub order_type: OrderType,
	/// Whether market is spot or perp
	pub market_type: MarketType,
	/// User generated order id. Can make it easier to place/cancel orders
	pub user_order_id: u8,
	/// Whether the user is buying or selling
	pub side: OrderSide,
	/// Whether the order is allowed to only reduce position size
	pub reduce_only: bool,
	/// Whether the order must be a maker
	pub post_only: bool,
	/// Whether the order must be canceled the same slot it is placed
	pub immediate_or_cancel: bool,
	/// Whether the order is triggered above or below the trigger price. Only relevant for trigger orders
	pub trigger_condition: OrderTriggerCondition,
	/// How many slots the auction lasts
	pub auction_duration: u8,
	pub padding: [u8; 3],
}

#[derive(Clone, Copy, BorshSerialize, BorshDeserialize, PartialEq, Eq, Debug)]
pub enum AssetType {
	Base,
	Quote,
}

impl Order {
	pub fn seconds_til_expiry(self, now: i64) -> i64 {
		(self.max_ts - now).max(0)
	}

	pub fn get_limit_price(
		&self,
		valid_oracle_price: Option<i64>,
		fallback_price: Option<u64>,
		slot: u64,
		tick_size: u64
	) -> NormalResult<Option<u64>> {
		let price = if
			self.has_auction_price(self.slot, self.auction_duration, slot)?
		{
			Some(calculate_auction_price(self, slot, tick_size, valid_oracle_price)?)
		} else if self.price == 0 {
			match fallback_price {
				Some(price) => Some(standardize_price(price, tick_size, self.side)?),
				None => None,
			}
		} else {
			Some(self.price)
		};

		Ok(price)
	}

	#[track_caller]
	#[inline(always)]
	pub fn force_get_limit_price(
		&self,
		valid_oracle_price: Option<i64>,
		fallback_price: Option<u64>,
		slot: u64,
		tick_size: u64
	) -> NormalResult<u64> {
		match
			self.get_limit_price(valid_oracle_price, fallback_price, slot, tick_size)?
		{
			Some(price) => Ok(price),
			None => {
				let caller = Location::caller();
				msg!(
					"Could not get limit price at {}:{}",
					caller.file(),
					caller.line()
				);
				Err(ErrorCode::UnableToGetLimitPrice)
			}
		}
	}

	pub fn has_limit_price(self, slot: u64) -> NormalResult<bool> {
		Ok(
			self.price > 0 ||
				!is_auction_complete(self.slot, self.auction_duration, slot)?
		)
	}

	pub fn is_auction_complete(self, slot: u64) -> NormalResult<bool> {
		is_auction_complete(self.slot, self.auction_duration, slot)
	}

	pub fn has_auction(&self) -> bool {
		self.auction_duration != 0
	}

	pub fn has_auction_price(
		&self,
		order_slot: u64,
		auction_duration: u8,
		slot: u64
	) -> NormalResult<bool> {
		let auction_complete = is_auction_complete(
			order_slot,
			auction_duration,
			slot
		)?;
		let has_auction_prices =
			self.auction_start_price != 0 || self.auction_end_price != 0;
		Ok(!auction_complete && has_auction_prices)
	}

	/// Passing in an existing_position forces the function to consider the order's reduce only status
	pub fn get_base_asset_amount_unfilled(
		&self,
		existing_position: Option<u64>
	) -> NormalResult<u64> {
		let base_asset_amount_unfilled = self.base_asset_amount.safe_sub(
			self.base_asset_amount_filled
		)?;

		let existing_position = match existing_position {
			Some(existing_position) => existing_position,
			None => {
				return Ok(base_asset_amount_unfilled);
			}
		};

		// if order is post only, can disregard reduce only
		if !self.reduce_only || self.post_only {
			return Ok(base_asset_amount_unfilled);
		}

		if existing_position == 0 {
			return Ok(0);
		}

		match self.side {
			OrderSide::Buy => {
				if existing_position > 0 {
					Ok(0)
				} else {
					Ok(base_asset_amount_unfilled.min(existing_position.unsigned_abs()))
				}
			}
			OrderSide::Sell => {
				if existing_position < 0 {
					Ok(0)
				} else {
					Ok(base_asset_amount_unfilled.min(existing_position.unsigned_abs()))
				}
			}
		}
	}

	/// Stardardizes the base asset amount unfilled to the nearest step size
	/// Particularly important for spot positions where existing position can be dust
	pub fn get_standardized_base_asset_amount_unfilled(
		&self,
		existing_position: Option<i64>,
		step_size: u64
	) -> NormalResult<u64> {
		standardize_base_asset_amount(
			self.get_base_asset_amount_unfilled(existing_position)?,
			step_size
		)
	}

	pub fn must_be_triggered(&self) -> bool {
		matches!(
			self.order_type,
			OrderType::TriggerMarket | OrderType::TriggerLimit
		)
	}

	pub fn triggered(&self) -> bool {
		matches!(
			self.trigger_condition,
			OrderTriggerCondition::TriggeredAbove |
				OrderTriggerCondition::TriggeredBelow
		)
	}

	pub fn is_jit_maker(&self) -> bool {
		self.post_only && self.immediate_or_cancel
	}

	pub fn is_open_order_for_market(
		&self,
		market_index: u16,
		market_type: &MarketType
	) -> bool {
		self.market_index == market_index &&
			self.status == OrderStatus::Open &&
			&self.market_type == market_type
	}

	pub fn is_market_order(&self) -> bool {
		matches!(self.order_type, OrderType::Market | OrderType::TriggerMarket)
	}

	pub fn is_limit_order(&self) -> bool {
		matches!(self.order_type, OrderType::Limit | OrderType::TriggerLimit)
	}

	pub fn is_resting_limit_order(&self, slot: u64) -> NormalResult<bool> {
		if !self.is_limit_order() {
			return Ok(false);
		}

		if self.order_type == OrderType::TriggerLimit {
			return match self.side {
				OrderSide::Buy if self.trigger_price < self.price => {
					return Ok(false);
				}
				OrderSide::Sell if self.trigger_price > self.price => {
					return Ok(false);
				}
				_ => self.is_auction_complete(slot),
			};
		}

		Ok(self.post_only || self.is_auction_complete(slot)?)
	}
}

impl Default for Order {
	fn default() -> Self {
		Self {
			status: OrderStatus::Init,
			order_type: OrderType::Limit,
			market_type: MarketType::Synthetic,
			slot: 0,
			order_id: 0,
			user_order_id: 0,
			market_index: 0,
			price: 0,
			base_asset_amount: 0,
			base_asset_amount_filled: 0,
			quote_asset_amount_filled: 0,
			side: OrderSide::Buy,
			reduce_only: false,
			post_only: false,
			immediate_or_cancel: false,
			trigger_price: 0,
			trigger_condition: OrderTriggerCondition::Above,
			auction_start_price: 0,
			auction_end_price: 0,
			auction_duration: 0,
			max_ts: 0,
			padding: [0; 3],
		}
	}
}

#[derive(Clone, Copy, BorshSerialize, BorshDeserialize, PartialEq, Eq, Debug)]
pub enum OrderStatus {
	/// The order is not in use
	Init,
	/// Order is open
	Open,
	/// Order has been filled
	Filled,
	/// Order has been canceled
	Canceled,
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
pub enum OrderType {
	Market,
	#[default]
	Limit,
	TriggerMarket,
	TriggerLimit,
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
pub enum OrderTriggerCondition {
	#[default]
	Above,
	Below,
	TriggeredAbove, // above condition has been triggered
	TriggeredBelow, // below condition has been triggered
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
	Synthetic,
}

impl fmt::Display for MarketType {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			MarketType::Synthetic => write!(f, "Synthetic"),
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

	/// Insurance
	///
	/// The amount of tokens staked in the quote spot markets if
	pub insurance_fund_staked_amount: u64,

	/// Rolling 30day maker volume for user
	/// precision: QUOTE_PRECISION
	pub maker_volume_30d: u64,
	/// Rolling 30day taker volume for user
	/// precision: QUOTE_PRECISION
	pub taker_volume_30d: u64,
	/// Rolling 30day filler volume for user
	/// precision: QUOTE_PRECISION
	pub filler_volume_30d: u64,
	/// last time the maker volume was updated
	pub last_maker_volume_30d_ts: i64,
	/// last time the taker volume was updated
	pub last_taker_volume_30d_ts: i64,
	/// last time the filler volume was updated
	pub last_filler_volume_30d_ts: i64,

	/// The current number of sub accounts
	pub number_of_sub_accounts: u16,
	/// The number of sub accounts created. Can be greater than the number of sub accounts if user
	/// has deleted sub accounts
	pub number_of_sub_accounts_created: u16,
	/// Whether the user is a referrer. Sub account 0 can not be deleted if user is a referrer
	pub is_referrer: bool,
	pub disable_update_bid_ask_twap: bool,
	pub padding1: [u8; 2],

	pub padding: [u8; 12],
}

impl Size for UserStats {
	const SIZE: usize = 240;
}

impl UserStats {
	pub fn update_maker_volume_30d(
		&mut self,
		quote_asset_amount: u64,
		now: i64
	) -> NormalResult {
		let since_last = max(1_i64, now.safe_sub(self.last_maker_volume_30d_ts)?);

		self.maker_volume_30d = calculate_rolling_sum(
			self.maker_volume_30d,
			quote_asset_amount,
			since_last,
			THIRTY_DAY
		)?;
		self.last_maker_volume_30d_ts = now;

		Ok(())
	}

	pub fn update_taker_volume_30d(
		&mut self,
		quote_asset_amount: u64,
		now: i64
	) -> NormalResult {
		let since_last = max(1_i64, now.safe_sub(self.last_taker_volume_30d_ts)?);

		self.taker_volume_30d = calculate_rolling_sum(
			self.taker_volume_30d,
			quote_asset_amount,
			since_last,
			THIRTY_DAY
		)?;
		self.last_taker_volume_30d_ts = now;

		Ok(())
	}

	pub fn update_filler_volume(
		&mut self,
		quote_asset_amount: u64,
		now: i64
	) -> NormalResult {
		let since_last = max(1_i64, now.safe_sub(self.last_filler_volume_30d_ts)?);

		self.filler_volume_30d = calculate_rolling_sum(
			self.filler_volume_30d,
			quote_asset_amount,
			since_last,
			THIRTY_DAY
		)?;

		self.last_filler_volume_30d_ts = now;

		Ok(())
	}

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
