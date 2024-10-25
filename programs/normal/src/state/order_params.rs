use crate::controller::position::OrderSide;
use crate::error::{ NormalResult, ErrorCode };
use crate::math::casting::Cast;
use crate::math::safe_math::SafeMath;
use crate::math::safe_unwrap::SafeUnwrap;
use crate::state::events::OrderActionExplanation;
use crate::state::market::{ SyntheticTier, Market };
use crate::state::user::{ MarketType, OrderTriggerCondition, OrderType };
use crate::{
	ONE_HUNDRED_THOUSAND_QUOTE,
	PERCENTAGE_PRECISION_I64,
	PERCENTAGE_PRECISION_U64,
	PRICE_PRECISION_I64,
};
use anchor_lang::prelude::*;
use borsh::{ BorshDeserialize, BorshSerialize };
use std::ops::Div;

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
pub enum OrderSide {
	#[default]
	Buy,
	Sell,
}

impl OrderSide {
	pub fn opposite(&self) -> Self {
		match self {
			OrderSide::Buy => OrderSide::Sell,
			OrderSide::Sell => OrderSide::Buy,
		}
	}
}

#[derive(
	AnchorSerialize,
	AnchorDeserialize,
	Clone,
	Default,
	Copy,
	Eq,
	PartialEq,
	Debug
)]
pub struct OrderParams {
	pub order_type: OrderType,
	pub market_type: MarketType,
	pub side: OrderSide,
	pub user_order_id: u8,
	pub base_asset_amount: u64,
	pub price: u64,
	pub market_index: u16,
	pub reduce_only: bool,
	pub post_only: PostOnlyParam,
	pub immediate_or_cancel: bool,
	pub max_ts: Option<i64>,
	pub trigger_price: Option<u64>,
	pub trigger_condition: OrderTriggerCondition,
	pub auction_duration: Option<u8>, // specified in slots
	pub auction_start_price: Option<i64>, // specified in price
	pub auction_end_price: Option<i64>, // specified in price
}

impl OrderParams {
	pub fn update_auction_params_limit_orders(
		&mut self,
		market: &Market,
		oracle_price: i64
	) -> NormalResult {
		if self.post_only != PostOnlyParam::None {
			return Ok(());
		}

		if self.price == 0 {
			return Ok(());
		}

		let auction_start_price_offset =
			OrderParams::get_baseline_start_price_offset(market, self.side)?;
		let mut new_auction_start_price = oracle_price.safe_add(
			auction_start_price_offset
		)?;

		if self.auction_duration.unwrap_or(0) == 0 {
			match self.side {
				OrderSide::Buy => {
					let ask_premium = market.amm.last_ask_premium()?;
					let est_ask = oracle_price.safe_add(ask_premium)?.cast()?;
					if self.price <= est_ask {
						// if auction duration is empty and limit doesnt cross vamm premium, return early
						return Ok(());
					} else {
						let new_auction_start_price = new_auction_start_price.min(
							est_ask as i64
						);
						msg!("Updating auction start price to {}", new_auction_start_price);
						self.auction_start_price = Some(new_auction_start_price);
						msg!("Updating auction end price to {}", self.price);
						self.auction_end_price = Some(self.price as i64);
					}
				}
				OrderSide::Sell => {
					let bid_discount = market.amm.last_bid_discount()?;
					let est_bid = oracle_price.safe_sub(bid_discount)?.cast()?;
					if self.price >= est_bid {
						// if auction duration is empty and limit doesnt cross vamm discount, return early
						return Ok(());
					} else {
						let new_auction_start_price = new_auction_start_price.max(
							est_bid as i64
						);
						msg!("Updating auction start price to {}", new_auction_start_price);
						self.auction_start_price = Some(new_auction_start_price);
						msg!("Updating auction end price to {}", self.price);
						self.auction_end_price = Some(self.price as i64);
					}
				}
			}
		} else {
			match self.auction_start_price {
				Some(auction_start_price) => {
					let improves_long =
						self.side == OrderSide::Buy &&
						new_auction_start_price < auction_start_price;

					let improves_short =
						self.side == OrderSide::Sell &&
						new_auction_start_price > auction_start_price;

					if improves_long || improves_short {
						msg!("Updating auction start price to {}", new_auction_start_price);
						self.auction_start_price = Some(new_auction_start_price);
					}
				}
				None => {
					msg!("Updating auction start price to {}", new_auction_start_price);
					self.auction_start_price = Some(new_auction_start_price);
				}
			}

			if self.auction_end_price.is_none() {
				msg!("Updating auction end price to {}", self.price);
				self.auction_end_price = Some(self.price as i64);
			}
		}

		let auction_duration_before = self.auction_duration;
		let new_auction_duration = get_auction_duration(
			self.auction_end_price
				.safe_unwrap()?
				.safe_sub(self.auction_start_price.safe_unwrap()?)?
				.unsigned_abs(),
			oracle_price.unsigned_abs(),
			market.contract_tier
		)?;
		self.auction_duration = Some(
			auction_duration_before.unwrap_or(0).max(new_auction_duration)
		);

		if auction_duration_before != self.auction_duration {
			msg!(
				"Updating auction duration to {}",
				self.auction_duration.safe_unwrap()?
			);
		}

		Ok(())
	}

	pub fn get_auction_start_price_offset(
		self,
		oracle_price: i64
	) -> NormalResult<i64> {
		let start_offset = if
			let Some(auction_start_price) = self.auction_start_price
		{
			auction_start_price.safe_sub(oracle_price)?
		} else {
			return Ok(0);
		};

		Ok(start_offset)
	}

	pub fn get_auction_end_price_offset(
		self,
		oracle_price: i64
	) -> NormalResult<i64> {
		let end_offset = if let Some(auction_end_price) = self.auction_end_price {
			auction_end_price.safe_sub(oracle_price)?
		} else {
			return Ok(0);
		};

		Ok(end_offset)
	}

	pub fn update_auction_params_market_orders(
		&mut self,
		market: &Market,
		oracle_price: i64
	) -> NormalResult {
		if
			self.auction_duration.is_none() ||
			self.auction_start_price.is_none() ||
			self.auction_end_price.is_none()
		{
			let (auction_start_price, auction_end_price, auction_duration) = {
				OrderParams::derive_market_order_auction_params(
					market,
					self.side,
					oracle_price,
					self.price,
					PERCENTAGE_PRECISION_I64 / 400 // 25 bps
				)?
			};

			self.auction_start_price = Some(auction_start_price);
			self.auction_end_price = Some(auction_end_price);
			self.auction_duration = Some(auction_duration);

			msg!(
				"Updating auction start price to {}",
				self.auction_start_price.safe_unwrap()?
			);

			msg!(
				"Updating auction end price to {}",
				self.auction_end_price.safe_unwrap()?
			);

			msg!(
				"Updating auction duration to {}",
				self.auction_duration.safe_unwrap()?
			);

			return Ok(());
		}
		// only update auction start price if the contract tier isn't Isolated
		if market.can_sanitize_market_order_auctions() {
			let (new_start_price_offset, new_end_price_offset) =
				OrderParams::get_baseline_start_end_price_offset(market, self.side, 2)?;
			let current_start_price_offset =
				self.get_auction_start_price_offset(oracle_price)?;
			let current_end_price_offset =
				self.get_auction_end_price_offset(oracle_price)?;
			match self.side {
				OrderSide::Buy => {
					if current_start_price_offset > new_start_price_offset {
						self.auction_start_price = if !is_market_order {
							Some(new_start_price_offset)
						} else {
							Some(new_start_price_offset.safe_add(oracle_price)?)
						};
						msg!(
							"Updating auction start price to {}",
							self.auction_start_price.safe_unwrap()?
						);
					}

					if current_end_price_offset > new_end_price_offset {
						self.auction_end_price = if !is_market_order {
							Some(new_end_price_offset)
						} else {
							Some(new_end_price_offset.safe_add(oracle_price)?)
						};
						msg!(
							"Updating auction end price to {}",
							self.auction_end_price.safe_unwrap()?
						);
					}
				}
				OrderSide::Sell => {
					if current_start_price_offset < new_start_price_offset {
						self.auction_start_price = if !is_market_order {
							Some(new_start_price_offset)
						} else {
							Some(new_start_price_offset.safe_add(oracle_price)?)
						};
						msg!(
							"Updating auction start price to {}",
							self.auction_start_price.safe_unwrap()?
						);
					}

					if current_end_price_offset < new_end_price_offset {
						self.auction_end_price = if !is_market_order {
							Some(new_end_price_offset)
						} else {
							Some(new_end_price_offset.safe_add(oracle_price)?)
						};
						msg!(
							"Updating auction end price to {}",
							self.auction_end_price.safe_unwrap()?
						);
					}
				}
			}
		}

		let auction_duration_before = self.auction_duration;
		let new_auction_duration = get_auction_duration(
			self.auction_end_price
				.safe_unwrap()?
				.safe_sub(self.auction_start_price.safe_unwrap()?)?
				.unsigned_abs(),
			oracle_price.unsigned_abs(),
			market.contract_tier
		)?;
		self.auction_duration = Some(
			auction_duration_before.unwrap_or(0).max(new_auction_duration)
		);

		if auction_duration_before != self.auction_duration {
			msg!(
				"Updating auction duration to {}",
				self.auction_duration.safe_unwrap()?
			);
		}

		Ok(())
	}

	pub fn derive_market_order_auction_params(
		market: &Market,
		side: OrderSide,
		oracle_price: i64,
		limit_price: u64,
		start_buffer: i64
	) -> NormalResult<(i64, i64, u8)> {
		let (mut auction_start_price, mut auction_end_price) = if limit_price != 0 {
			let (auction_start_price_offset, auction_end_price_offset) =
				OrderParams::get_baseline_start_end_price_offset(market, side, 2)?;
			let mut auction_start_price = oracle_price.safe_add(
				auction_start_price_offset
			)?;
			let mut auction_end_price = oracle_price.safe_add(
				auction_end_price_offset
			)?;

			let limit_price = limit_price as i64;
			if side == OrderSide::Buy {
				auction_start_price = auction_start_price.min(limit_price);
				auction_end_price = auction_end_price.min(limit_price);
			} else {
				auction_start_price = auction_start_price.max(limit_price);
				auction_end_price = auction_end_price.max(limit_price);
			}

			(auction_start_price, auction_end_price)
		} else {
			let (auction_start_price_offset, auction_end_price_offset) =
				OrderParams::get_baseline_start_end_price_offset(market, side, 1)?;
			let auction_start_price = oracle_price.safe_add(
				auction_start_price_offset
			)?;
			let auction_end_price = oracle_price.safe_add(auction_end_price_offset)?;

			(auction_start_price, auction_end_price)
		};

		if start_buffer != 0 {
			let start_buffer_price = oracle_price
				.safe_mul(start_buffer)?
				.safe_div(PERCENTAGE_PRECISION_I64)?;

			if side == OrderSide::Buy {
				auction_start_price = auction_start_price.safe_sub(start_buffer_price)?;
			} else {
				auction_start_price = auction_start_price.safe_add(start_buffer_price)?;
			}
		}

		let auction_duration = get_auction_duration(
			auction_end_price.safe_sub(auction_start_price)?.unsigned_abs(),
			oracle_price.unsigned_abs(),
			market.contract_tier
		)?;

		Ok((auction_start_price, auction_end_price, auction_duration))
	}

	pub fn update_auction_params(
		&mut self,
		market: &Market,
		oracle_price: i64
	) -> NormalResult {
		#[cfg(feature = "anchor-test")]
		return Ok(());

		match self.order_type {
			OrderType::Limit => {
				self.update_auction_params_limit_orders(market, oracle_price)?;
			}
			OrderType::Market => {
				self.update_auction_params_market_orders(market, oracle_price)?;
			}
			_ => {}
		}

		Ok(())
	}

	pub fn get_baseline_start_price_offset(
		market: &Market,
		side: OrderSide
	) -> NormalResult<i64> {
		if
			market.amm.historical_oracle_data.last_oracle_price_twap_ts
				.safe_sub(market.amm.last_mark_price_twap_ts)?
				.abs() >= 60 ||
			market.amm.volume_24h <= ONE_HUNDRED_THOUSAND_QUOTE
		{
			// if uncertain with timestamp mismatch, enforce within N bps
			let price_divisor = if
				market.contract_tier.is_as_safe_as_contract(&SyntheticTier::B)
			{
				500
			} else {
				100
			};

			return Ok(match side {
				OrderSide::Buy => {
					market.amm.last_bid_price_twap.cast::<i64>()? / price_divisor
				}
				OrderSide::Sell => {
					-(market.amm.last_ask_price_twap.cast::<i64>()? / price_divisor)
				}
			});
		}

		// price offsets baselines for market auctions
		let mark_twap_slow = (
			match side {
				OrderSide::Buy => market.amm.last_bid_price_twap,
				OrderSide::Sell => market.amm.last_ask_price_twap,
			}
		).cast::<i64>()?;

		let baseline_start_price_offset_slow = mark_twap_slow.safe_sub(
			market.amm.historical_oracle_data.last_oracle_price_twap
		)?;

		let baseline_start_price_offset_fast = market.amm.last_mark_price_twap_5min
			.cast::<i64>()?
			.safe_sub(market.amm.historical_oracle_data.last_oracle_price_twap_5min)?;

		let frac_of_buy_spread_in_price: i64 = market.amm.buy_spread
			.cast::<i64>()?
			.safe_mul(mark_twap_slow)?
			.safe_div(PRICE_PRECISION_I64 * 10)?;

		let frac_of_sell_spread_in_price: i64 = market.amm.sell_spread
			.cast::<i64>()?
			.safe_mul(mark_twap_slow)?
			.safe_div(PRICE_PRECISION_I64 * 10)?;

		let baseline_start_price_offset = match side {
			OrderSide::Buy =>
				baseline_start_price_offset_slow
					.safe_add(frac_of_buy_spread_in_price)?
					.min(
						baseline_start_price_offset_fast.safe_sub(
							frac_of_sell_spread_in_price
						)?
					),
			OrderSide::Sell =>
				baseline_start_price_offset_slow
					.safe_sub(frac_of_sell_spread_in_price)?
					.max(
						baseline_start_price_offset_fast.safe_add(
							frac_of_buy_spread_in_price
						)?
					),
		};

		Ok(baseline_start_price_offset)
	}

	pub fn get_baseline_start_end_price_offset(
		market: &Market,
		side: OrderSide,
		end_buffer_scalar: u64
	) -> NormalResult<(i64, i64)> {
		let oracle_twap =
			market.amm.historical_oracle_data.last_oracle_price_twap.unsigned_abs();
		let baseline_start_price_offset =
			OrderParams::get_baseline_start_price_offset(market, side)?;
		let (min_divisor, max_divisor) = market.get_auction_end_min_max_divisors()?;

		let amm_spread_side_pct = if side == OrderSide::Sell {
			market.amm.sell_spread
		} else {
			market.amm.buy_spread
		};

		let mut baseline_end_price_buffer = market.amm.mark_std
			.max(market.amm.oracle_std)
			.max(
				amm_spread_side_pct
					.cast::<u64>()?
					.safe_mul(oracle_twap)?
					.safe_div(PERCENTAGE_PRECISION_U64)?
			);
		if end_buffer_scalar >= 1 {
			baseline_end_price_buffer =
				baseline_end_price_buffer.safe_mul(end_buffer_scalar)?;
		}
		baseline_end_price_buffer = baseline_end_price_buffer.clamp(
			oracle_twap / min_divisor,
			oracle_twap / max_divisor
		);

		let baseline_end_price_offset = if side == OrderSide::Sell {
			let auction_end_price = market.amm.last_bid_price_twap
				.safe_sub(baseline_end_price_buffer)?
				.cast::<i64>()?
				.safe_sub(market.amm.historical_oracle_data.last_oracle_price_twap)?;
			auction_end_price.min(baseline_start_price_offset)
		} else {
			let auction_end_price = market.amm.last_ask_price_twap
				.safe_add(baseline_end_price_buffer)?
				.cast::<i64>()?
				.safe_sub(market.amm.historical_oracle_data.last_oracle_price_twap)?;

			auction_end_price.max(baseline_start_price_offset)
		};

		Ok((baseline_start_price_offset, baseline_end_price_offset))
	}

	pub fn get_close_params(
		market: &Market,
		side_to_close: OrderSide,
		base_asset_amount: u64
	) -> NormalResult<OrderParams> {
		let (auction_start_price, auction_end_price) =
			OrderParams::get_baseline_start_end_price_offset(
				market,
				side_to_close,
				1
			)?;

		let params = OrderParams {
			market_type: MarketType::Synthetic,
			side: side_to_close,
			order_type: OrderType::Market, // TODO: used to be Oracle, unsure why?
			market_index: market.market_index,
			base_asset_amount,
			reduce_only: true,
			auction_start_price: Some(auction_start_price),
			auction_end_price: Some(auction_end_price),
			auction_duration: Some(80),
			..OrderParams::default()
		};

		Ok(params)
	}
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Eq, PartialEq, Debug)]
pub struct SwiftServerMessage {
	pub swift_order_signature: [u8; 64],
	pub slot: u64,
}

#[derive(
	AnchorSerialize,
	AnchorDeserialize,
	Clone,
	Default,
	Eq,
	PartialEq,
	Debug
)]
pub struct SwiftOrderParamsMessage {
	pub swift_order_params: OrderParams,
	pub expected_order_id: i32,
	pub sub_account_id: u16,
	pub take_profit_order_params: Option<SwiftTriggerOrderParams>,
	pub stop_loss_order_params: Option<SwiftTriggerOrderParams>,
}

#[derive(
	AnchorSerialize,
	AnchorDeserialize,
	Clone,
	Default,
	Eq,
	PartialEq,
	Debug
)]
pub struct SwiftTriggerOrderParams {
	pub trigger_price: u64,
	pub base_asset_amount: u64,
}

fn get_auction_duration(
	price_diff: u64,
	price: u64,
	synthetic_tier: SyntheticTier
) -> NormalResult<u8> {
	let percent_diff = price_diff.safe_mul(PERCENTAGE_PRECISION_U64)?.div(price);

	let slots_per_bp = if
		synthetic_tier.is_as_safe_as_contract(&SyntheticTier::B)
	{
		100
	} else {
		60
	};

	Ok(
		percent_diff
			.safe_mul(slots_per_bp)?
			.safe_div_ceil(PERCENTAGE_PRECISION_U64 / 100)
			? // 1% = 60 slots
			.clamp(10, 180) as u8
	) // 180 slots max
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
pub enum PostOnlyParam {
	#[default]
	None,
	MustPostOnly, // Tx fails if order can't be post only
	TryPostOnly, // Tx succeeds and order not placed if can't be post only
	Slide, // Modify price to be post only if can't be post only
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Default)]
pub struct ModifyOrderParams {
	pub side: Option<OrderSide>,
	pub base_asset_amount: Option<u64>,
	pub price: Option<u64>,
	pub reduce_only: Option<bool>,
	pub post_only: Option<PostOnlyParam>,
	pub immediate_or_cancel: Option<bool>,
	pub max_ts: Option<i64>,
	pub trigger_price: Option<u64>,
	pub trigger_condition: Option<OrderTriggerCondition>,
	pub auction_duration: Option<u8>,
	pub auction_start_price: Option<i64>,
	pub auction_end_price: Option<i64>,
	pub policy: Option<ModifyOrderPolicy>,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Eq, PartialEq)]
pub enum ModifyOrderPolicy {
	TryModify,
	MustModify,
}

impl Default for ModifyOrderPolicy {
	fn default() -> Self {
		Self::TryModify
	}
}

pub struct PlaceOrderOptions {
	pub swift_taker_order_slot: Option<u64>,
	pub try_expire_orders: bool,
	pub risk_increasing: bool,
	pub explanation: OrderActionExplanation,
}

impl Default for PlaceOrderOptions {
	fn default() -> Self {
		Self {
			swift_taker_order_slot: None,
			try_expire_orders: true,
			risk_increasing: false,
			explanation: OrderActionExplanation::None,
		}
	}
}

impl PlaceOrderOptions {
	pub fn update_risk_increasing(&mut self, risk_increasing: bool) {
		self.risk_increasing = self.risk_increasing || risk_increasing;
	}

	pub fn explanation(mut self, explanation: OrderActionExplanation) -> Self {
		self.explanation = explanation;
		self
	}

	pub fn set_order_slot(&mut self, slot: u64) {
		self.swift_taker_order_slot = Some(slot);
	}

	pub fn get_order_slot(&self, order_slot: u64) -> u64 {
		let mut min_order_slot = order_slot;
		if let Some(swift_taker_order_slot) = self.swift_taker_order_slot {
			min_order_slot = order_slot.min(swift_taker_order_slot);
		}
		min_order_slot
	}
}

pub enum PlaceAndTakeOrderSuccessCondition {
	PartialFill = 1,
	FullFill = 2,
}
