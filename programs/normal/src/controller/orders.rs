use std::cell::RefMut;
use std::collections::BTreeMap;
use std::ops::DerefMut;
use std::u64;

use anchor_lang::prelude::*;
use solana_program::msg;

use crate::controller;
use crate::controller::position;
use crate::controller::position::{
	add_new_position,
	decrease_open_bids_and_asks,
	get_position_index,
	increase_open_bids_and_asks,
	update_position_and_market,
	update_quote_asset_amount,
	OrderSide,
};
use crate::error::NormalResult;
use crate::error::ErrorCode;
use crate::get_struct_values;
use crate::get_then_update_id;
use crate::load_mut;
use crate::math::amm_jit::calculate_amm_jit_liquidity;
use crate::math::auction::{
	calculate_auction_params_for_trigger_order,
	calculate_auction_prices,
};
use crate::math::casting::Cast;
use crate::constants::constants::{
	BASE_PRECISION_U64,
	PERP_DECIMALS,
	QUOTE_SPOT_MARKET_INDEX,
};
use crate::math::fees::{ determine_user_fee_tier, FillFees };
use crate::math::fulfillment::{ determine_fulfillment_methods };
use crate::math::matching::{
	are_orders_same_market_but_different_sides,
	calculate_fill_for_matched_orders,
	calculate_filler_multiplier_for_matched_orders,
	do_orders_cross,
	is_maker_for_taker,
};
use crate::math::oracle::{
	is_oracle_valid_for_action,
	NormalAction,
	OracleValidity,
};
use crate::math::safe_math::SafeMath;
use crate::math::balance::{ get_signed_token_amount, get_token_amount };
use crate::math::{ amm, fees, orders::* };
use crate::state::order_params::{
	ModifyOrderParams,
	ModifyOrderPolicy,
	OrderParams,
	PlaceOrderOptions,
	PostOnlyParam,
};

use crate::math::amm::calculate_amm_available_liquidity;
use crate::math::safe_unwrap::SafeUnwrap;
use crate::print_error;
use crate::state::events::{
	emit_stack,
	get_order_action_record,
	LPAction,
	LPRecord,
	OrderActionRecord,
	OrderRecord,
};
use crate::state::events::{ OrderAction, OrderActionExplanation };
use crate::state::fill_mode::FillMode;
use crate::state::fulfillment::{ FulfillmentMethod };
use crate::state::oracle::{ OraclePriceData, StrictOraclePrice };
use crate::state::oracle_map::OracleMap;
use crate::state::paused_operations::{ Operation };
use crate::state::market::{ MarketStatus, Market };
use crate::state::amm::AMMLiquiditySplit;
use crate::state::market::{ Market };
use crate::state::market_map::MarketMap;
use crate::state::state::FeeStructure;
use crate::state::state::*;
use crate::state::traits::Size;
use crate::state::user::{
	Order,
	OrderStatus,
	OrderTriggerCondition,
	OrderType,
	UserStats,
};
use crate::state::user::{ MarketType, User };
use crate::state::user_map::{ UserMap, UserStatsMap };
use crate::validate;
use crate::validation;
use crate::validation::order::{
	validate_order,
	validate_order_for_force_reduce_only,
};

pub fn place_order(
	state: &State,
	user: &mut User,
	user_key: Pubkey,
	market_map: &MarketMap,
	oracle_map: &mut OracleMap,
	clock: &Clock,
	mut params: OrderParams,
	mut options: PlaceOrderOptions
) -> NormalResult {
	let now = clock.unix_timestamp;
	let slot: u64 = clock.slot;

	if options.try_expire_orders {
		expire_orders(user, &user_key, market_map, oracle_map, now, slot)?;
	}

	if user.is_reduce_only() {
		validate!(
			params.reduce_only,
			ErrorCode::UserReduceOnly,
			"order must be reduce only"
		)?;
	}

	let new_order_index = user.orders
		.iter()
		.position(|order| order.status.eq(&OrderStatus::Init))
		.ok_or(ErrorCode::MaxNumberOfOrders)?;

	if params.user_order_id > 0 {
		let user_order_id_already_used = user.orders
			.iter()
			.position(|order| order.user_order_id == params.user_order_id);

		if user_order_id_already_used.is_some() {
			msg!("user_order_id is already in use {}", params.user_order_id);
			return Err(ErrorCode::UserOrderIdAlreadyInUse);
		}
	}

	let market_index = params.market_index;
	let market = &market_map.get_ref(&market_index)?;
	let force_reduce_only = market.is_reduce_only()?;

	validate!(
		!matches!(market.status, MarketStatus::Initialized),
		ErrorCode::MarketBeingInitialized,
		"Market is being initialized"
	)?;

	validate!(
		!market.is_in_settlement(now),
		ErrorCode::MarketPlaceOrderPaused,
		"Market is in settlement mode"
	)?;

	let position_index = get_position_index(
		&user.positions,
		market_index
	).or_else(|_| add_new_position(&mut user.positions, market_index))?;

	// Increment open orders for existing position
	let order_base_asset_amount = {
		validate!(
			params.base_asset_amount >= market.amm.order_step_size,
			ErrorCode::OrderAmountTooSmall,
			"params.base_asset_amount={} cannot be below market.amm.order_step_size={}",
			params.base_asset_amount,
			market.amm.order_step_size
		)?;

		let base_asset_amount = if params.base_asset_amount == u64::MAX {
			calculate_max_order_size(
				user,
				position_index,
				params.market_index,
				params.side,
				market_map,
				oracle_map
			)?
		} else {
			standardize_base_asset_amount(
				params.base_asset_amount,
				market.amm.order_step_size
			)?
		};

		base_asset_amount
	};

	let oracle_price_data = oracle_map.get_price_data(&market.amm.oracle)?;

	// updates auction params for crossing limit orders w/out auction duration
	params.update_auction_params(market, oracle_price_data.price)?;

	let (auction_start_price, auction_end_price, auction_duration) =
		get_auction_params(
			&params,
			oracle_price_data,
			market.amm.order_tick_size,
			state.min_auction_duration
		)?;

	let max_ts = match params.max_ts {
		Some(max_ts) => max_ts,
		None =>
			match params.order_type {
				OrderType::Market =>
					now.safe_add(
						(30_i64).max(
							auction_duration.safe_div(2)?.cast::<i64>()?.safe_add(10_i64)?
						)
					)?,
				_ => 0_i64,
			}
	};

	if max_ts != 0 && max_ts < now {
		msg!("max_ts ({}) < now ({}), skipping order", max_ts, now);
		return Ok(());
	}

	validate!(
		params.market_type == MarketType::Synthetic,
		ErrorCode::InvalidOrderMarketType,
		"must be synthetic order"
	)?;

	let new_order = Order {
		status: OrderStatus::Open,
		order_type: params.order_type,
		market_type: params.market_type,
		slot: options.get_order_slot(slot),
		order_id: get_then_update_id!(user, next_order_id),
		user_order_id: params.user_order_id,
		market_index: params.market_index,
		price: get_price_for_order(
			params.price,
			params.side,
			params.post_only,
			&market.amm
		)?,
		base_asset_amount: order_base_asset_amount,
		base_asset_amount_filled: 0,
		quote_asset_amount_filled: 0,
		side: params.side,
		reduce_only: params.reduce_only || force_reduce_only,
		trigger_price: standardize_price(
			params.trigger_price.unwrap_or(0),
			market.amm.order_tick_size,
			params.side
		)?,
		trigger_condition: params.trigger_condition,
		post_only: params.post_only != PostOnlyParam::None,
		immediate_or_cancel: params.immediate_or_cancel,
		auction_start_price,
		auction_end_price,
		auction_duration,
		max_ts,
		padding: [0; 3],
	};

	let valid_oracle_price = Some(
		oracle_map.get_price_data(&market.amm.oracle)?.price
	);
	match validate_order(&new_order, market, valid_oracle_price, slot) {
		Ok(()) => {}
		Err(ErrorCode::PlacePostOnlyLimitFailure) if
			params.post_only == PostOnlyParam::TryPostOnly
		=> {
			// just want place to succeeds without error if TryPostOnly
			return Ok(());
		}
		Err(err) => {
			return Err(err);
		}
	}

	let risk_increasing = is_new_order_risk_increasing(
		&new_order,
		user.positions[position_index].base_asset_amount,
		user.positions[position_index].open_bids,
		user.positions[position_index].open_asks
	)?;

	user.increment_open_orders(new_order.has_auction());
	user.orders[new_order_index] = new_order;
	user.positions[position_index].open_orders += 1;
	if !new_order.must_be_triggered() {
		increase_open_bids_and_asks(
			&mut user.positions[position_index],
			&params.side,
			order_base_asset_amount
		)?;
	}

	options.update_risk_increasing(risk_increasing);

	if force_reduce_only {
		validate_order_for_force_reduce_only(
			&user.orders[new_order_index],
			user.positions[position_index].base_asset_amount
		)?;
	}

	let max_oi = market.amm.max_open_interest;
	if max_oi != 0 && risk_increasing {
		let oi_plus_order = match params.side {
			OrderSide::Buy =>
				market.amm.base_asset_amount_long
					.safe_add(order_base_asset_amount.cast()?)?
					.unsigned_abs(),
			OrderSide::Sell =>
				market.amm.base_asset_amount_long
					.safe_sub(order_base_asset_amount.cast()?)?
					.unsigned_abs(),
		};

		validate!(
			oi_plus_order <= max_oi,
			ErrorCode::MaxOpenInterest,
			"Order Base Amount={} could breach Max Open Interest for Market={}",
			order_base_asset_amount,
			params.market_index
		)?;
	}

	let (taker, taker_order, maker, maker_order) =
		get_taker_and_maker_for_order_record(&user_key, &new_order);

	let order_action_record = get_order_action_record(
		now,
		OrderAction::Place,
		options.explanation,
		market_index,
		None,
		None,
		None,
		None,
		None,
		None,
		None,
		None,
		None,
		None,
		taker,
		taker_order,
		maker,
		maker_order,
		oracle_map.get_price_data(&market.amm.oracle)?.price
	)?;
	emit_stack::<_, { OrderActionRecord::SIZE }>(order_action_record)?;

	let order_record = OrderRecord {
		ts: now,
		user: user_key,
		order: user.orders[new_order_index],
	};
	emit_stack::<_, { OrderRecord::SIZE }>(order_record)?;

	user.update_last_active_slot(slot);

	Ok(())
}

fn get_auction_params(
	params: &OrderParams,
	oracle_price_data: &OraclePriceData,
	tick_size: u64,
	min_auction_duration: u8
) -> NormalResult<(i64, i64, u8)> {
	if !matches!(params.order_type, OrderType::Market | OrderType::Limit) {
		return Ok((0_i64, 0_i64, 0_u8));
	}

	if params.order_type == OrderType::Limit {
		return match
			(
				params.auction_start_price,
				params.auction_end_price,
				params.auction_duration,
			)
		{
			(
				Some(auction_start_price),
				Some(auction_end_price),
				Some(auction_duration),
			) => {
				let auction_duration = if auction_duration == 0 {
					auction_duration
				} else {
					// if auction is non-zero, force it to be at least min_auction_duration
					auction_duration.max(min_auction_duration)
				};

				Ok((
					standardize_price_i64(
						auction_start_price,
						tick_size.cast()?,
						params.side
					)?,
					standardize_price_i64(
						auction_end_price,
						tick_size.cast()?,
						params.side
					)?,
					auction_duration,
				))
			}
			_ => Ok((0_i64, 0_i64, 0_u8)),
		};
	}

	let auction_duration = params.auction_duration
		.unwrap_or(0)
		.max(min_auction_duration);

	let (auction_start_price, auction_end_price) = match
		(params.auction_start_price, params.auction_end_price)
	{
		(Some(auction_start_price), Some(auction_end_price)) => {
			(auction_start_price, auction_end_price)
		}
		_ =>
			calculate_auction_prices(oracle_price_data, params.side, params.price)?,
	};

	Ok((
		standardize_price_i64(auction_start_price, tick_size.cast()?, params.side)?,
		standardize_price_i64(auction_end_price, tick_size.cast()?, params.side)?,
		auction_duration,
	))
}

pub fn cancel_orders(
	user: &mut User,
	user_key: &Pubkey,
	filler_key: Option<&Pubkey>,
	market_map: &MarketMap,
	oracle_map: &mut OracleMap,
	now: i64,
	slot: u64,
	explanation: OrderActionExplanation,
	market_type: Option<MarketType>,
	market_index: Option<u16>,
	side: Option<OrderSide>
) -> NormalResult<Vec<u32>> {
	let mut canceled_order_ids: Vec<u32> = vec![];
	for order_index in 0..user.orders.len() {
		if user.orders[order_index].status != OrderStatus::Open {
			continue;
		}

		if
			let (Some(market_type), Some(market_index)) = (market_type, market_index)
		{
			if user.orders[order_index].market_type != market_type {
				continue;
			}

			if user.orders[order_index].market_index != market_index {
				continue;
			}
		}

		if let Some(side) = side {
			if user.orders[order_index].side != side {
				continue;
			}
		}

		canceled_order_ids.push(user.orders[order_index].order_id);
		cancel_order(
			order_index,
			user,
			user_key,
			market_map,
			oracle_map,
			now,
			slot,
			explanation,
			filler_key,
			0,
			false
		)?;
	}

	user.update_last_active_slot(slot);

	Ok(canceled_order_ids)
}

pub fn cancel_order_by_order_id(
	order_id: u32,
	user: &AccountLoader<User>,
	market_map: &MarketMap,
	oracle_map: &mut OracleMap,
	clock: &Clock
) -> NormalResult {
	let user_key = user.key();
	let user = &mut load_mut!(user)?;
	let order_index = match user.get_order_index(order_id) {
		Ok(order_index) => order_index,
		Err(_) => {
			msg!("could not find order id {}", order_id);
			return Ok(());
		}
	};

	cancel_order(
		order_index,
		user,
		&user_key,
		market_map,
		oracle_map,
		clock.unix_timestamp,
		clock.slot,
		OrderActionExplanation::None,
		None,
		0,
		false
	)?;

	user.update_last_active_slot(clock.slot);

	Ok(())
}

pub fn cancel_order_by_user_order_id(
	user_order_id: u8,
	user: &AccountLoader<User>,
	market_map: &MarketMap,
	oracle_map: &mut OracleMap,
	clock: &Clock
) -> NormalResult {
	let user_key = user.key();
	let user = &mut load_mut!(user)?;
	let order_index = match
		user.orders.iter().position(|order| order.user_order_id == user_order_id)
	{
		Some(order_index) => order_index,
		None => {
			msg!("could not find user order id {}", user_order_id);
			return Ok(());
		}
	};

	cancel_order(
		order_index,
		user,
		&user_key,
		market_map,
		oracle_map,
		clock.unix_timestamp,
		clock.slot,
		OrderActionExplanation::None,
		None,
		0,
		false
	)?;

	user.update_last_active_slot(clock.slot);

	Ok(())
}

pub fn cancel_order(
	order_index: usize,
	user: &mut User,
	user_key: &Pubkey,
	market_map: &MarketMap,
	oracle_map: &mut OracleMap,
	now: i64,
	_slot: u64,
	explanation: OrderActionExplanation,
	filler_key: Option<&Pubkey>,
	filler_reward: u64,
	skip_log: bool
) -> NormalResult {
	let (order_status, order_market_index, order_side, order_market_type) =
		get_struct_values!(
			user.orders[order_index],
			status,
			market_index,
			side,
			market_type
		);

	validate!(order_status == OrderStatus::Open, ErrorCode::OrderNotOpen)?;

	let oracle = market_map.get_ref(&order_market_index)?.amm.oracle;

	if !skip_log {
		let (taker, taker_order, maker, maker_order) =
			get_taker_and_maker_for_order_record(user_key, &user.orders[order_index]);

		let order_action_record = get_order_action_record(
			now,
			OrderAction::Cancel,
			explanation,
			order_market_index,
			filler_key.copied(),
			None,
			Some(filler_reward),
			None,
			None,
			None,
			None,
			None,
			None,
			None,
			taker,
			taker_order,
			maker,
			maker_order,
			oracle_map.get_price_data(&oracle)?.price
		)?;
		emit_stack::<_, { OrderActionRecord::SIZE }>(order_action_record)?;
	}

	user.decrement_open_orders(user.orders[order_index].has_auction());

	// Decrement open orders for existing position
	let position_index = get_position_index(&user.positions, order_market_index)?;

	// only decrease open/bids ask if it's not a trigger order or if it's been triggered
	if
		!user.orders[order_index].must_be_triggered() ||
		user.orders[order_index].triggered()
	{
		let base_asset_amount_unfilled =
			user.orders[order_index].get_base_asset_amount_unfilled(None)?;
		position::decrease_open_bids_and_asks(
			&mut user.positions[position_index],
			&order_side,
			base_asset_amount_unfilled.cast()?
		)?;
	}

	user.positions[position_index].open_orders -= 1;
	user.orders[order_index] = Order::default();

	Ok(())
}

pub enum ModifyOrderId {
	UserOrderId(u8),
	OrderId(u32),
}

pub fn modify_order(
	order_id: ModifyOrderId,
	modify_order_params: ModifyOrderParams,
	user_loader: &AccountLoader<User>,
	state: &State,
	market_map: &MarketMap,
	oracle_map: &mut OracleMap,
	clock: &Clock
) -> NormalResult {
	let user_key = user_loader.key();
	let mut user = load_mut!(user_loader)?;

	let order_index = match order_id {
		ModifyOrderId::UserOrderId(user_order_id) => {
			match user.get_order_index_by_user_order_id(user_order_id) {
				Ok(order_index) => order_index,
				Err(e) => {
					msg!("User order id {} not found", user_order_id);
					if modify_order_params.policy == Some(ModifyOrderPolicy::MustModify) {
						return Err(e);
					} else {
						return Ok(());
					}
				}
			}
		}
		ModifyOrderId::OrderId(order_id) =>
			match user.get_order_index(order_id) {
				Ok(order_index) => order_index,
				Err(e) => {
					msg!("Order id {} not found", order_id);
					if modify_order_params.policy == Some(ModifyOrderPolicy::MustModify) {
						return Err(e);
					} else {
						return Ok(());
					}
				}
			}
	};

	let existing_order = user.orders[order_index];

	cancel_order(
		order_index,
		&mut user,
		&user_key,
		market_map,
		oracle_map,
		clock.unix_timestamp,
		clock.slot,
		OrderActionExplanation::None,
		None,
		0,
		false
	)?;

	user.update_last_active_slot(clock.slot);

	let order_params = merge_modify_order_params_with_existing_order(
		&existing_order,
		&modify_order_params
	)?;

	place_order(
		state,
		&mut user,
		user_key,
		market_map,
		oracle_map,
		clock,
		order_params,
		PlaceOrderOptions::default()
	)?;

	Ok(())
}

fn merge_modify_order_params_with_existing_order(
	existing_order: &Order,
	modify_order_params: &ModifyOrderParams
) -> NormalResult<OrderParams> {
	let order_type = existing_order.order_type;
	let market_type = existing_order.market_type;
	let side = modify_order_params.side.unwrap_or(existing_order.side);
	let user_order_id = existing_order.user_order_id;
	let base_asset_amount = modify_order_params.base_asset_amount.unwrap_or(
		existing_order.get_base_asset_amount_unfilled(None)?
	);
	let price = modify_order_params.price.unwrap_or(existing_order.price);
	let market_index = existing_order.market_index;
	let reduce_only = modify_order_params.reduce_only.unwrap_or(
		existing_order.reduce_only
	);
	let post_only = modify_order_params.post_only.unwrap_or(
		if existing_order.post_only {
			PostOnlyParam::MustPostOnly
		} else {
			PostOnlyParam::None
		}
	);
	let immediate_or_cancel = false;
	let max_ts = modify_order_params.max_ts.or(Some(existing_order.max_ts));
	let trigger_price = modify_order_params.trigger_price.or(
		Some(existing_order.trigger_price)
	);
	let trigger_condition = modify_order_params.trigger_condition.unwrap_or(match
		existing_order.trigger_condition
	{
		OrderTriggerCondition::TriggeredAbove | OrderTriggerCondition::Above => {
			OrderTriggerCondition::Above
		}
		OrderTriggerCondition::TriggeredBelow | OrderTriggerCondition::Below => {
			OrderTriggerCondition::Below
		}
	});

	let (auction_duration, auction_start_price, auction_end_price) = if
		modify_order_params.auction_duration.is_some() &&
		modify_order_params.auction_start_price.is_some() &&
		modify_order_params.auction_end_price.is_some()
	{
		(
			modify_order_params.auction_duration,
			modify_order_params.auction_start_price,
			modify_order_params.auction_end_price,
		)
	} else {
		(None, None, None)
	};

	Ok(OrderParams {
		order_type,
		market_type,
		side,
		user_order_id,
		base_asset_amount,
		price,
		market_index,
		reduce_only,
		post_only,
		immediate_or_cancel,
		max_ts,
		trigger_price,
		trigger_condition,
		auction_duration,
		auction_start_price,
		auction_end_price,
	})
}

pub fn fill_order(
	order_id: u32,
	state: &State,
	user: &AccountLoader<User>,
	user_stats: &AccountLoader<UserStats>,
	market_map: &MarketMap,
	oracle_map: &mut OracleMap,
	filler: &AccountLoader<User>,
	filler_stats: &AccountLoader<UserStats>,
	makers_and_referrer: &UserMap,
	makers_and_referrer_stats: &UserStatsMap,
	jit_maker_order_id: Option<u32>,
	clock: &Clock,
	fill_mode: FillMode
) -> NormalResult<(u64, u64)> {
	let now = clock.unix_timestamp;
	let slot = clock.slot;

	let filler_key = filler.key();
	let user_key = user.key();
	let user = &mut load_mut!(user)?;
	let user_stats = &mut load_mut!(user_stats)?;

	let order_index = user.orders
		.iter()
		.position(|order| order.order_id == order_id)
		.ok_or_else(print_error!(ErrorCode::OrderDoesNotExist))?;

	let (order_status, market_index, order_market_type, order_side) =
		get_struct_values!(
			user.orders[order_index],
			status,
			market_index,
			market_type,
			side
		);

	validate!(
		order_market_type == MarketType::Synthetic,
		ErrorCode::InvalidOrderMarketType,
		"must be synthetic order"
	)?;

	// TODO: investigate
	// settle lp position so its tradeable
	let mut market = market_map.get_ref_mut(&market_index)?;
	// controller::lp::settle_lp(user, &user_key, &mut market, now)?;

	validate!(
		matches!(market.status, MarketStatus::Active | MarketStatus::ReduceOnly),
		ErrorCode::MarketFillOrderPaused,
		"Market not active"
	)?;

	validate!(
		!market.is_operation_paused(Operation::Fill),
		ErrorCode::MarketFillOrderPaused,
		"Market fills paused"
	)?;

	drop(market);

	validate!(
		order_status == OrderStatus::Open,
		ErrorCode::OrderNotOpen,
		"Order not open"
	)?;

	validate!(
		!user.orders[order_index].must_be_triggered() ||
			user.orders[order_index].triggered(),
		ErrorCode::OrderMustBeTriggeredFirst,
		"Order must be triggered first"
	)?;

	let reserve_price_before: u64;
	let oracle_validity: OracleValidity;
	let oracle_price: i64;
	let oracle_twap_5min: i64;
	let market_index: u16;

	let mut amm_is_available = !state.amm_paused()?;
	{
		let market = &mut market_map.get_ref_mut(&market_index)?;
		validation::market::validate_market(market)?;
		validate!(
			!market.is_in_settlement(now),
			ErrorCode::MarketFillOrderPaused,
			"Market is in settlement mode"
		)?;

		let (oracle_price_data, _oracle_validity) =
			oracle_map.get_price_data_and_validity(
				MarketType::Synthetic,
				market.market_index,
				&market.amm.oracle,
				market.amm.historical_oracle_data.last_oracle_price_twap,
				market.get_max_confidence_interval_multiplier()?
			)?;

		amm_is_available &= is_oracle_valid_for_action(
			_oracle_validity,
			Some(NormalAction::FillOrderAmm)
		)?;
		amm_is_available &= !market.is_operation_paused(Operation::AmmFill);

		reserve_price_before = market.amm.reserve_price()?;
		oracle_price = oracle_price_data.price;
		oracle_twap_5min =
			market.amm.historical_oracle_data.last_oracle_price_twap_5min;
		oracle_validity = _oracle_validity;
		market_index = market.market_index;
	}

	// allow oracle price to be used to calculate limit price if it's valid or stale for amm
	let valid_oracle_price = if
		is_oracle_valid_for_action(
			oracle_validity,
			Some(NormalAction::OracleOrderPrice)
		)?
	{
		Some(oracle_price)
	} else {
		msg!("Market = {} oracle deemed invalid", market_index);
		None
	};

	let is_filler_taker = user_key == filler_key;
	let is_filler_maker = makers_and_referrer.0.contains_key(&filler_key);
	let (mut filler, mut filler_stats) = if !is_filler_maker && !is_filler_taker {
		let filler = load_mut!(filler)?;
		if filler.authority != user.authority {
			(Some(filler), Some(load_mut!(filler_stats)?))
		} else {
			(None, None)
		}
	} else {
		(None, None)
	};

	let maker_orders_info = get_maker_orders_info(
		market_map,
		oracle_map,
		makers_and_referrer,
		&user_key,
		&user.orders[order_index],
		&mut filler.as_deref_mut(),
		&filler_key,
		state.fee_structure.flat_filler_fee,
		oracle_price,
		jit_maker_order_id,
		now,
		slot
	)?;

	let referrer_info = get_referrer_info(
		user_stats,
		&user_key,
		makers_and_referrer,
		makers_and_referrer_stats,
		slot
	)?;

	let oracle_too_divergent_with_twap_5min =
		is_oracle_too_divergent_with_twap_5min(
			oracle_price,
			oracle_twap_5min,
			state.oracle_guard_rails.max_oracle_twap_5min_percent_divergence().cast()?
		)?;

	if oracle_too_divergent_with_twap_5min {
		// update filler last active so tx doesn't revert
		if let Some(filler) = filler.as_deref_mut() {
			filler.update_last_active_slot(slot);
		}

		return Ok((0, 0));
	}

	validate_fill_possible(
		state,
		user,
		order_index,
		slot,
		makers_and_referrer.0.len(),
		fill_mode
	)?;

	let should_expire_order = should_expire_order_before_fill(
		user,
		order_index,
		now
	)?;

	let position_index = get_position_index(
		&user.positions,
		user.orders[order_index].market_index
	)?;
	let existing_base_asset_amount = user.positions
		[position_index].base_asset_amount;
	let should_cancel_reduce_only = should_cancel_reduce_only_order(
		&user.orders[order_index],
		existing_base_asset_amount,
		market_map.get_ref_mut(&market_index)?.amm.order_step_size
	)?;

	if should_expire_order || should_cancel_reduce_only {
		let filler_reward = {
			let mut market = market_map.get_ref_mut(&market_index)?;
			pay_keeper_flat_reward(
				user,
				filler.as_deref_mut(),
				market.deref_mut(),
				state.fee_structure.flat_filler_fee,
				slot
			)?
		};

		let explanation = if should_expire_order {
			OrderActionExplanation::OrderExpired
		} else {
			OrderActionExplanation::ReduceOnlyOrderIncreasedPosition
		};

		cancel_order(
			order_index,
			user,
			&user_key,
			Operation,
			oracle_map,
			now,
			slot,
			explanation,
			Some(&filler_key),
			filler_reward,
			false
		)?;

		return Ok((0, 0));
	}

	let (base_asset_amount, quote_asset_amount) = fulfill_order(
		user,
		order_index,
		&user_key,
		user_stats,
		makers_and_referrer,
		makers_and_referrer_stats,
		&maker_orders_info,
		&mut filler.as_deref_mut(),
		&filler_key,
		&mut filler_stats.as_deref_mut(),
		referrer_info,
		market_map,
		oracle_map,
		&state.fee_structure,
		reserve_price_before,
		valid_oracle_price,
		now,
		slot,
		state.min_auction_duration,
		amm_is_available,
		fill_mode
	)?;

	if base_asset_amount != 0 {
		let fill_price = calculate_fill_price(
			quote_asset_amount,
			base_asset_amount,
			BASE_PRECISION_U64
		)?;

		let market = market_map.get_ref(&market_index)?;
		validate_fill_price_within_price_bands(
			fill_price,
			order_side,
			oracle_price,
			oracle_twap_5min,
			state.oracle_guard_rails.max_oracle_twap_5min_percent_divergence()
		)?;
	}

	let base_asset_amount_after = user.positions
		[position_index].base_asset_amount;
	let should_cancel_reduce_only = should_cancel_reduce_only_order(
		&user.orders[order_index],
		base_asset_amount_after,
		market_map.get_ref_mut(&market_index)?.amm.order_step_size
	)?;

	if should_cancel_reduce_only {
		let filler_reward = {
			let mut market = market_map.get_ref_mut(&market_index)?;
			pay_keeper_flat_reward(
				user,
				filler.as_deref_mut(),
				market.deref_mut(),
				state.fee_structure.flat_filler_fee,
				slot
			)?
		};

		let explanation = OrderActionExplanation::ReduceOnlyOrderIncreasedPosition;

		cancel_order(
			order_index,
			user,
			&user_key,
			market_map,
			oracle_map,
			now,
			slot,
			explanation,
			Some(&filler_key),
			filler_reward,
			false
		)?;
	}

	if base_asset_amount == 0 {
		return Ok((base_asset_amount, quote_asset_amount));
	}

	{
		let market = market_map.get_ref(&market_index)?;

		let open_interest = market.get_open_interest(0, valid_oracle_price);
		let max_open_interest = market.amm.max_open_interest;

		validate!(
			max_open_interest == 0 || max_open_interest > open_interest,
			ErrorCode::MaxOpenInterest,
			"open interest ({}) > max open interest ({})",
			open_interest,
			max_open_interest
		)?;
	}

	let total_open_interest = 0;
	for (_key, market_account_loader) in market_map.0.iter_mut() {
		let market = &mut load_mut!(market_account_loader)?;
		let open_interest = market.get_open_interest();
		total_open_interest = total_open_interest.safe_add(open_interest);
	}

	let insurance_fund = &mut load_mut!(market.insurance_fund);
	insurance_fund.max_insurance = total_open_interest;

	user.update_last_active_slot(slot);

	Ok((base_asset_amount, quote_asset_amount))
}

pub fn validate_market_within_price_band(
	market: &Market,
	state: &State,
	oracle_price: i64
) -> NormalResult<bool> {
	let reserve_price = market.amm.reserve_price()?;

	let reserve_spread_pct = amm::calculate_oracle_twap_5min_price_spread_pct(
		&market.amm,
		reserve_price
	)?;

	let oracle_spread_pct = amm::calculate_oracle_twap_5min_price_spread_pct(
		&market.amm,
		oracle_price.unsigned_abs()
	)?;

	if reserve_spread_pct.abs() > oracle_spread_pct.abs() {
		let is_reserve_too_divergent = amm::is_oracle_mark_too_divergent(
			reserve_spread_pct,
			&state.oracle_guard_rails.price_divergence
		)?;

		// if oracle-mark divergence pushed outside limit, block order
		if is_reserve_too_divergent {
			msg!(
				"Market = {} price pushed outside bounds: last_oracle_price_twap_5min={} vs reserve_price={},(breach spread {})",
				market.market_index,
				market.amm.historical_oracle_data.last_oracle_price_twap_5min,
				reserve_price,
				reserve_spread_pct
			);
			return Err(ErrorCode::PriceBandsBreached);
		}
	} else {
		let is_oracle_too_divergent = amm::is_oracle_mark_too_divergent(
			oracle_spread_pct,
			&state.oracle_guard_rails.price_divergence
		)?;

		// if oracle-mark divergence pushed outside limit, block order
		if is_oracle_too_divergent {
			msg!(
				"Market = {} price pushed outside bounds: last_oracle_price_twap_5min={} vs oracle_price={},(breach spread {})",
				market.market_index,
				market.amm.historical_oracle_data.last_oracle_price_twap_5min,
				oracle_price,
				oracle_spread_pct
			);
			return Err(ErrorCode::PriceBandsBreached);
		}
	}

	Ok(true)
}

#[allow(clippy::type_complexity)]
fn get_maker_orders_info(
	market_map: &MarketMap,
	oracle_map: &mut OracleMap,
	makers_and_referrer: &UserMap,
	taker_key: &Pubkey,
	taker_order: &Order,
	filler: &mut Option<&mut User>,
	filler_key: &Pubkey,
	filler_reward: u64,
	oracle_price: i64,
	jit_maker_order_id: Option<u32>,
	now: i64,
	slot: u64
) -> NormalResult<Vec<(Pubkey, usize, u64)>> {
	let maker_side = taker_order.side.opposite();

	let mut maker_orders_info = Vec::with_capacity(16);

	for (maker_key, user_account_loader) in makers_and_referrer.0.iter() {
		if maker_key == taker_key {
			continue;
		}

		let mut maker = load_mut!(user_account_loader)?;

		let mut market = market_map.get_ref_mut(&taker_order.market_index)?;
		let maker_order_price_and_indexes = find_maker_orders(
			&maker,
			&maker_side,
			&MarketType::Synthetic,
			taker_order.market_index,
			Some(oracle_price),
			slot,
			market.amm.order_tick_size
		)?;

		if maker_order_price_and_indexes.is_empty() {
			continue;
		}

		maker.update_last_active_slot(slot);

		let step_size = market.amm.order_step_size;

		drop(market);

		for (
			maker_order_index,
			maker_order_price,
		) in maker_order_price_and_indexes.iter() {
			let maker_order_index = *maker_order_index;
			let maker_order_price = *maker_order_price;

			let maker_order = &maker.orders[maker_order_index];
			if !is_maker_for_taker(maker_order, taker_order, slot)? {
				continue;
			}

			if !are_orders_same_market_but_different_sides(maker_order, taker_order) {
				continue;
			}

			if let Some(jit_maker_order_id) = jit_maker_order_id {
				// if jit maker order id exists, must only use that order
				if maker_order.order_id != jit_maker_order_id {
					continue;
				}
			}

			let should_expire_order = should_expire_order(
				&maker,
				maker_order_index,
				now
			)?;

			let existing_base_asset_amount = maker.get_position(
				maker.orders[maker_order_index].market_index
			)?.base_asset_amount;
			let should_cancel_reduce_only_order = should_cancel_reduce_only_order(
				&maker.orders[maker_order_index],
				existing_base_asset_amount,
				step_size
			)?;

			if should_expire_order || should_cancel_reduce_only_order {
				let filler_reward = {
					let mut market = market_map.get_ref_mut(
						&maker.orders[maker_order_index].market_index
					)?;
					pay_keeper_flat_reward(
						&mut maker,
						filler.as_deref_mut(),
						market.deref_mut(),
						filler_reward,
						slot
					)?
				};

				let explanation = if should_expire_order {
					OrderActionExplanation::OrderExpired
				} else {
					OrderActionExplanation::ReduceOnlyOrderIncreasedPosition
				};

				cancel_order(
					maker_order_index,
					maker.deref_mut(),
					maker_key,
					market_map,
					oracle_map,
					now,
					slot,
					explanation,
					Some(filler_key),
					filler_reward,
					false
				)?;

				continue;
			}

			insert_maker_order_info(
				&mut maker_orders_info,
				(*maker_key, maker_order_index, maker_order_price),
				maker_side
			);
		}
	}

	Ok(maker_orders_info)
}

#[inline(always)]
fn insert_maker_order_info(
	maker_orders_info: &mut Vec<(Pubkey, usize, u64)>,
	maker_order_info: (Pubkey, usize, u64),
	side: OrderSide
) {
	let price = maker_order_info.2;
	let index = match
		maker_orders_info.binary_search_by(|item| {
			match side {
				OrderSide::Sell => item.2.cmp(&price),
				OrderSide::Buy => price.cmp(&item.2),
			}
		})
	{
		Ok(index) => index,
		Err(index) => index,
	};

	if index < maker_orders_info.capacity() {
		maker_orders_info.insert(index, maker_order_info);
	}
}

fn get_referrer_info(
	user_stats: &UserStats,
	user_key: &Pubkey,
	makers_and_referrer: &UserMap,
	makers_and_referrer_stats: &UserStatsMap,
	slot: u64
) -> NormalResult<Option<(Pubkey, Pubkey)>> {
	if user_stats.referrer.eq(&Pubkey::default()) {
		return Ok(None);
	}

	validate!(
		makers_and_referrer_stats.0.contains_key(&user_stats.referrer),
		ErrorCode::ReferrerStatsNotFound
	)?;

	let referrer_authority_key = user_stats.referrer;
	let mut referrer_user_key = Pubkey::default();
	for (referrer_key, referrer) in makers_and_referrer.0.iter() {
		// if user is in makers and referrer map, skip to avoid invalid borrow
		if referrer_key == user_key {
			continue;
		}

		let mut referrer = load_mut!(referrer)?;
		if referrer.authority != referrer_authority_key {
			continue;
		}

		if referrer.sub_account_id == 0 {
			referrer.update_last_active_slot(slot);
			referrer_user_key = *referrer_key;
			break;
		}
	}

	if referrer_user_key == Pubkey::default() {
		return Err(ErrorCode::ReferrerNotFound);
	}

	Ok(Some((referrer_authority_key, referrer_user_key)))
}

fn fulfill_order(
	user: &mut User,
	user_order_index: usize,
	user_key: &Pubkey,
	user_stats: &mut UserStats,
	makers_and_referrer: &UserMap,
	makers_and_referrer_stats: &UserStatsMap,
	maker_orders_info: &[(Pubkey, usize, u64)],
	filler: &mut Option<&mut User>,
	filler_key: &Pubkey,
	filler_stats: &mut Option<&mut UserStats>,
	referrer_info: Option<(Pubkey, Pubkey)>,
	market_map: &MarketMap,
	oracle_map: &mut OracleMap,
	fee_structure: &FeeStructure,
	reserve_price_before: u64,
	valid_oracle_price: Option<i64>,
	now: i64,
	slot: u64,
	min_auction_duration: u8,
	amm_is_available: bool,
	fill_mode: FillMode
) -> NormalResult<(u64, u64)> {
	let market_index = user.orders[user_order_index].market_index;

	let user_order_position_decreasing =
		determine_if_user_order_is_position_decreasing(
			user,
			market_index,
			user_order_index
		)?;

	let market = market_map.get_ref(&market_index)?;
	let limit_price = fill_mode.get_limit_price(
		&user.orders[user_order_index],
		valid_oracle_price,
		slot,
		market.amm.order_tick_size
	)?;
	drop(market);

	let fulfillment_methods = {
		let market = market_map.get_ref(&market_index)?;
		let oracle_price = oracle_map.get_price_data(&market.amm.oracle)?.price;

		determine_fulfillment_methods(
			&user.orders[user_order_index],
			maker_orders_info,
			&market.amm,
			reserve_price_before,
			Some(oracle_price),
			limit_price,
			amm_is_available,
			slot,
			min_auction_duration,
			fill_mode
		)?
	};

	if fulfillment_methods.is_empty() {
		return Ok((0, 0));
	}

	let mut base_asset_amount = 0_u64;
	let mut quote_asset_amount = 0_u64;
	let mut maker_fills: BTreeMap<Pubkey, i64> = BTreeMap::new();
	let maker_side = user.orders[user_order_index].side.opposite();
	for fulfillment_method in fulfillment_methods.iter() {
		if user.orders[user_order_index].status != OrderStatus::Open {
			break;
		}
		let mut market = market_map.get_ref_mut(&market_index)?;
		let user_order_side: OrderSide = user.orders[user_order_index].side;

		let (fill_base_asset_amount, fill_quote_asset_amount) = match
			fulfillment_method
		{
			FulfillmentMethod::AMM(maker_price) => {
				let (mut referrer, mut referrer_stats) = get_referrer(
					&referrer_info,
					makers_and_referrer,
					makers_and_referrer_stats,
					None
				)?;

				// maker may try to fill their own order (e.g. via jit)
				// if amm takes fill, give maker filler reward
				let (mut maker, mut maker_stats) = if
					makers_and_referrer.0.contains_key(filler_key) &&
					filler.is_none()
				{
					let maker = makers_and_referrer.get_ref_mut(filler_key)?;
					if maker.authority == user.authority {
						(None, None)
					} else {
						let maker_stats = makers_and_referrer_stats.get_ref_mut(
							&maker.authority
						)?;
						(Some(maker), Some(maker_stats))
					}
				} else {
					(None, None)
				};

				let (fill_base_asset_amount, fill_quote_asset_amount) =
					fulfill_order_with_amm(
						user,
						user_stats,
						user_order_index,
						market.deref_mut(),
						oracle_map,
						reserve_price_before,
						now,
						slot,
						user_key,
						filler_key,
						filler,
						filler_stats,
						&mut maker.as_deref_mut(),
						&mut maker_stats.as_deref_mut(),
						&mut referrer.as_deref_mut(),
						&mut referrer_stats.as_deref_mut(),
						fee_structure,
						limit_price,
						None,
						*maker_price,
						AMMLiquiditySplit::Shared
					)?;

				(fill_base_asset_amount, fill_quote_asset_amount)
			}
			FulfillmentMethod::Match(maker_key, maker_order_index) => {
				let mut maker = makers_and_referrer.get_ref_mut(maker_key)?;
				let mut maker_stats = if maker.authority == user.authority {
					None
				} else {
					Some(makers_and_referrer_stats.get_ref_mut(&maker.authority)?)
				};

				let (mut referrer, mut referrer_stats) = get_referrer(
					&referrer_info,
					makers_and_referrer,
					makers_and_referrer_stats,
					Some(&maker)
				)?;

				let (
					fill_base_asset_amount,
					fill_quote_asset_amount,
					maker_fill_base_asset_amount,
				) = fulfill_order_with_match(
					market.deref_mut(),
					user,
					user_stats,
					user_order_index,
					user_key,
					&mut maker,
					&mut maker_stats.as_deref_mut(),
					*maker_order_index as usize,
					maker_key,
					filler,
					filler_stats,
					filler_key,
					&mut referrer.as_deref_mut(),
					&mut referrer_stats.as_deref_mut(),
					reserve_price_before,
					valid_oracle_price,
					limit_price,
					now,
					slot,
					fee_structure,
					oracle_map
				)?;

				if maker_fill_base_asset_amount != 0 {
					update_maker_fills_map(
						&mut maker_fills,
						maker_key,
						maker_side,
						maker_fill_base_asset_amount
					)?;
				}

				(fill_base_asset_amount, fill_quote_asset_amount)
			}
		};

		base_asset_amount = base_asset_amount.safe_add(fill_base_asset_amount)?;
		quote_asset_amount = quote_asset_amount.safe_add(fill_quote_asset_amount)?;
		market.amm.update_volume_24h(
			fill_quote_asset_amount,
			user_order_side,
			now
		)?;
	}

	validate!(
		(base_asset_amount > 0) == (quote_asset_amount > 0),
		ErrorCode::DefaultError,
		"invalid fill base = {} quote = {}",
		base_asset_amount,
		quote_asset_amount
	)?;

	let total_maker_fill = maker_fills.values().sum::<i64>();

	validate!(
		total_maker_fill.unsigned_abs() <= base_asset_amount,
		ErrorCode::DefaultError,
		"invalid total maker fill {} total fill {}",
		total_maker_fill,
		base_asset_amount
	)?;

	Ok((base_asset_amount, quote_asset_amount))
}

#[allow(clippy::type_complexity)]
fn get_referrer<'a>(
	referrer_info: &'a Option<(Pubkey, Pubkey)>,
	makers_and_referrer: &'a UserMap,
	makers_and_referrer_stats: &'a UserStatsMap,
	maker: Option<&User>
) -> NormalResult<(Option<RefMut<'a, User>>, Option<RefMut<'a, UserStats>>)> {
	let (referrer_authority_key, referrer_user_key) = match referrer_info {
		Some(referrer_keys) => referrer_keys,
		None => {
			return Ok((None, None));
		}
	};

	if let Some(maker) = maker {
		if &maker.authority == referrer_authority_key {
			return Ok((None, None));
		}
	}

	let referrer = makers_and_referrer.get_ref_mut(referrer_user_key)?;
	let referrer_stats = makers_and_referrer_stats.get_ref_mut(
		referrer_authority_key
	)?;

	Ok((Some(referrer), Some(referrer_stats)))
}

#[inline(always)]
fn update_maker_fills_map(
	map: &mut BTreeMap<Pubkey, i64>,
	maker_key: &Pubkey,
	maker_side: OrderSide,
	fill: u64
) -> NormalResult {
	let signed_fill = match maker_side {
		OrderSide::Buy => fill.cast::<i64>()?,
		OrderSide::Sell => -fill.cast::<i64>()?,
	};

	if let Some(maker_filled) = map.get_mut(maker_key) {
		*maker_filled = maker_filled.safe_add(signed_fill)?;
	} else {
		map.insert(*maker_key, signed_fill);
	}

	Ok(())
}

fn determine_if_user_order_is_position_decreasing(
	user: &User,
	market_index: u16,
	order_index: usize
) -> NormalResult<bool> {
	let position_index = get_position_index(&user.positions, market_index)?;
	let order_side = user.orders[order_index].side;
	let position_base_asset_amount_before = user.positions
		[position_index].base_asset_amount;
	is_order_position_reducing(
		&order_side,
		user.orders[order_index].get_base_asset_amount_unfilled(
			Some(position_base_asset_amount_before)
		)?,
		position_base_asset_amount_before.cast()?
	)
}

/// TODO: update to exchange tokens directly b/t AMM and taker and mint tokens if reserve is empty
pub fn fulfill_order_with_amm(
	user: &mut User,
	user_stats: &mut UserStats,
	order_index: usize,
	market: &mut Market,
	oracle_map: &mut OracleMap,
	reserve_price_before: u64,
	now: i64,
	slot: u64,
	user_key: &Pubkey,
	filler_key: &Pubkey,
	filler: &mut Option<&mut User>,
	filler_stats: &mut Option<&mut UserStats>,
	maker: &mut Option<&mut User>,
	maker_stats: &mut Option<&mut UserStats>,
	referrer: &mut Option<&mut User>,
	referrer_stats: &mut Option<&mut UserStats>,
	fee_structure: &FeeStructure,
	limit_price: Option<u64>,
	override_base_asset_amount: Option<u64>,
	override_fill_price: Option<u64>,
	liquidity_split: AMMLiquiditySplit
) -> NormalResult<(u64, u64)> {
	let position_index = get_position_index(
		&user.positions,
		market.market_index
	)?;
	let existing_base_asset_amount = user.positions
		[position_index].base_asset_amount;

	// Determine the base asset amount the market can fill
	let (base_asset_amount, limit_price, fill_price) = match
		override_base_asset_amount
	{
		Some(override_base_asset_amount) => {
			(override_base_asset_amount, limit_price, override_fill_price)
		}
		None => {
			let fee_tier = determine_user_fee_tier(
				user_stats,
				fee_structure,
				&MarketType::Synthetic
			)?;
			let (base_asset_amount, limit_price) =
				calculate_base_asset_amount_for_amm_to_fulfill(
					&user.orders[order_index],
					market,
					limit_price,
					override_fill_price,
					existing_base_asset_amount,
					fee_tier
				)?;

			let fill_price = if user.orders[order_index].post_only {
				limit_price
			} else {
				None
			};

			(base_asset_amount, limit_price, fill_price)
		}
	};

	// if user position is less than min order size, step size is the threshold
	let amm_size_threshold = if
		existing_base_asset_amount.unsigned_abs() > market.amm.min_order_size
	{
		market.amm.min_order_size
	} else {
		market.amm.order_step_size
	};

	if base_asset_amount < amm_size_threshold {
		// if is an actual swap (and not amm jit order) then msg!
		if override_base_asset_amount.is_none() {
			msg!(
				"Amm cant fulfill order. market index {} base asset amount {} market.amm.min_order_size {}",
				market.market_index,
				base_asset_amount,
				market.amm.min_order_size
			);
		}
		return Ok((0, 0));
	}

	let (order_post_only, order_slot, order_side) = get_struct_values!(
		user.orders[order_index],
		post_only,
		slot,
		side
	);

	validation::market::validate_amm_account_for_fill(&market.amm, order_side)?;

	let market_side_price = match order_side {
		OrderSide::Buy => market.amm.ask_price(reserve_price_before)?,
		OrderSide::Sell => market.amm.bid_price(reserve_price_before)?,
	};

	let sanitize_clamp_denominator = market.get_sanitize_clamp_denominator()?;
	amm::update_mark_twap_from_estimates(
		&mut market.amm,
		now,
		Some(market_side_price),
		Some(order_side),
		sanitize_clamp_denominator
	)?;

	let (quote_asset_amount, quote_asset_amount_surplus, _) =
		controller::position::update_position_with_base_asset_amount(
			base_asset_amount,
			order_side,
			market,
			user,
			position_index,
			fill_price
		)?;

	if let Some(limit_price) = limit_price {
		validate_fill_price(
			quote_asset_amount,
			base_asset_amount,
			BASE_PRECISION_U64,
			order_side,
			limit_price,
			!order_post_only
		)?;
	}

	let reward_referrer = can_reward_user_with_pnl(referrer, market.market_index);
	let reward_filler =
		can_reward_user_with_pnl(filler, market.market_index) ||
		can_reward_user_with_pnl(maker, market.market_index);

	let FillFees {
		user_fee,
		fee_to_market,
		filler_reward,
		referee_discount,
		referrer_reward,
		fee_to_market_for_lp,
		maker_rebate,
	} = fees::calculate_fee_for_fulfillment_with_amm(
		user_stats,
		quote_asset_amount,
		fee_structure,
		order_slot,
		slot,
		reward_filler,
		reward_referrer,
		referrer_stats,
		quote_asset_amount_surplus,
		order_post_only,
		market.fee_adjustment
	)?;

	let user_position_delta = get_position_delta_for_fill(
		base_asset_amount,
		quote_asset_amount,
		order_side
	)?;

	if liquidity_split != AMMLiquiditySplit::ProtocolOwned {
		// TODO: anything to update here?
	}

	// TODO: do we need this logic if user_lp_shares has been removed?
	if amm.user_lp_shares > 0 {
		let (new_terminal_quote_reserve, new_terminal_base_reserve) =
			crate::math::amm::calculate_terminal_reserves(&market.amm)?;
		market.amm.terminal_quote_asset_reserve = new_terminal_quote_reserve;

		let (min_base_asset_reserve, max_base_asset_reserve) =
			crate::math::amm::calculate_bid_ask_bounds(
				market.amm.concentration_coef,
				new_terminal_base_reserve
			)?;
		market.amm.min_base_asset_reserve = min_base_asset_reserve;
		market.amm.max_base_asset_reserve = max_base_asset_reserve;
	}

	// Increment the protocol's total fee variables
	market.amm.total_fee = market.amm.total_fee.safe_add(fee_to_market.cast()?)?;
	market.amm.total_exchange_fee = market.amm.total_exchange_fee.safe_add(
		user_fee.cast()?
	)?;
	market.amm.total_mm_fee = market.amm.total_mm_fee.safe_add(
		quote_asset_amount_surplus.cast()?
	)?;
	market.amm.total_fee_minus_distributions =
		market.amm.total_fee_minus_distributions.safe_add(fee_to_market.cast()?)?;

	// Increment the user's total fee variables
	user_stats.increment_total_fees(user_fee)?;
	user_stats.increment_total_rebate(maker_rebate)?;
	user_stats.increment_total_referee_discount(referee_discount)?;

	if
		let (Some(referrer), Some(referrer_stats)) = (
			referrer.as_mut(),
			referrer_stats.as_mut(),
		)
	{
		if
			let Ok(referrer_position) = referrer.force_get_position_mut(
				market.market_index
			)
		{
			if referrer_reward > 0 {
				update_quote_asset_amount(
					referrer_position,
					market,
					referrer_reward.cast()?
				)?;
			}
			referrer_stats.increment_total_referrer_reward(referrer_reward, now)?;
		}
	}

	let position_index = get_position_index(
		&user.positions,
		market.market_index
	)?;

	if user_fee != 0 {
		controller::position::update_quote_asset_amount(
			&mut user.positions[position_index],
			market,
			-user_fee.cast()?
		)?;
	}

	if maker_rebate != 0 {
		controller::position::update_quote_asset_amount(
			&mut user.positions[position_index],
			market,
			maker_rebate.cast()?
		)?;
	}

	if order_post_only {
		user_stats.update_maker_volume_30d(quote_asset_amount, now)?;
	} else {
		user_stats.update_taker_volume_30d(quote_asset_amount, now)?;
	}

	if let Some(filler) = filler.as_mut() {
		credit_filler_pnl(
			filler,
			filler_stats,
			market,
			filler_reward,
			quote_asset_amount,
			now,
			slot
		)?;
	} else if let Some(maker) = maker.as_mut() {
		credit_filler_pnl(
			maker,
			maker_stats,
			market,
			filler_reward,
			quote_asset_amount,
			now,
			slot
		)?;
	}

	update_order_after_fill(
		&mut user.orders[order_index],
		base_asset_amount,
		quote_asset_amount
	)?;

	decrease_open_bids_and_asks(
		&mut user.positions[position_index],
		&order_side,
		base_asset_amount
	)?;

	let (taker, taker_order, maker, maker_order) =
		get_taker_and_maker_for_order_record(user_key, &user.orders[order_index]);

	let fill_record_id = get_then_update_id!(market, next_fill_record_id);
	let order_action_explanation = match
		(override_base_asset_amount, override_fill_price)
	{
		(Some(_), Some(_)) => liquidity_split.get_order_action_explanation(),
		_ => OrderActionExplanation::OrderFilledWithAMM,
	};
	let order_action_record = get_order_action_record(
		now,
		OrderAction::Fill,
		order_action_explanation,
		market.market_index,
		Some(*filler_key),
		Some(fill_record_id),
		Some(filler_reward),
		Some(base_asset_amount),
		Some(quote_asset_amount),
		Some(user_fee),
		if maker_rebate != 0 {
			Some(maker_rebate)
		} else {
			None
		},
		Some(referrer_reward),
		Some(quote_asset_amount_surplus),
		None,
		taker,
		taker_order,
		maker,
		maker_order,
		oracle_map.get_price_data(&market.amm.oracle)?.price
	)?;
	emit_stack::<_, { OrderActionRecord::SIZE }>(order_action_record)?;

	// Cant reset order until after its logged
	if user.orders[order_index].get_base_asset_amount_unfilled(None)? == 0 {
		user.decrement_open_orders(user.orders[order_index].has_auction());
		user.orders[order_index] = Order::default();
		let market_position = &mut user.positions[position_index];
		market_position.open_orders -= 1;
	}

	// TODO: custom token exchange - not sure where to place yet
	// 1. Send tokens from user to AMM
	controller::token::receive(
		token_program,
		user_key,
		market.amm.key,
		authority,
		amount,
		mint
	);

	if market.amm.base_asset_reserve == 0 {
		// 2a. If AMM has no tokens, mint them
		controller::token::mint_synthetic_tokens(
			&market.amm.token,
			user_key,
			&ctx.accounts.normal_signer,
			amount,
			&market.amm.token_mint
		);
	} else {
		// 2. Send tokens from AMM to user
		controller::token::send_from_program_vault(
			&market.amm.token,
			market.amm.key,
			user_key,
			&ctx.accounts.normal_signer,
			nonce,
			amount,
			&market.amm.token_mint
		);
	}

	Ok((base_asset_amount, quote_asset_amount))
}

pub fn credit_filler_pnl(
	filler: &mut User,
	filler_stats: &mut Option<&mut UserStats>,
	market: &mut Market,
	filler_reward: u64,
	quote_asset_amount: u64,
	now: i64,
	slot: u64
) -> NormalResult {
	if filler_reward > 0 {
		let position_index = get_position_index(
			&filler.positions,
			market.market_index
		).or_else(|_|
			add_new_position(&mut filler.positions, market.market_index)
		)?;

		controller::position::update_quote_asset_amount(
			&mut filler.positions[position_index],
			market,
			filler_reward.cast()?
		)?;

		filler_stats
			.as_mut()
			.safe_unwrap()?
			.update_filler_volume(quote_asset_amount, now)?;
	}

	filler.update_last_active_slot(slot);

	Ok(())
}

/// TODO: update to exchange tokens directly b/t maker and taker
pub fn fulfill_order_with_match(
	market: &mut Market,
	taker: &mut User,
	taker_stats: &mut UserStats,
	taker_order_index: usize,
	taker_key: &Pubkey,
	maker: &mut User,
	maker_stats: &mut Option<&mut UserStats>,
	maker_order_index: usize,
	maker_key: &Pubkey,
	filler: &mut Option<&mut User>,
	filler_stats: &mut Option<&mut UserStats>,
	filler_key: &Pubkey,
	referrer: &mut Option<&mut User>,
	referrer_stats: &mut Option<&mut UserStats>,
	reserve_price_before: u64,
	valid_oracle_price: Option<i64>,
	taker_limit_price: Option<u64>,
	now: i64,
	slot: u64,
	fee_structure: &FeeStructure,
	oracle_map: &mut OracleMap
) -> NormalResult<(u64, u64, u64)> {
	if
		!are_orders_same_market_but_different_sides(
			&maker.orders[maker_order_index],
			&taker.orders[taker_order_index]
		)
	{
		return Ok((0_u64, 0_u64, 0_u64));
	}

	let oracle_price = oracle_map.get_price_data(&market.amm.oracle)?.price;
	let taker_side: OrderSide = taker.orders[taker_order_index].side;

	let taker_price = if let Some(taker_limit_price) = taker_limit_price {
		taker_limit_price
	} else {
		let amm_available_liquidity = calculate_amm_available_liquidity(
			&market.amm,
			&taker_side
		)?;
		market.amm.get_fallback_price(
			&taker_side,
			amm_available_liquidity,
			oracle_price,
			taker.orders[taker_order_index].seconds_til_expiry(now)
		)?
	};

	let taker_existing_position = taker.get_position(
		market.market_index
	)?.base_asset_amount;
	let taker_base_asset_amount = taker.orders[
		taker_order_index
	].get_base_asset_amount_unfilled(Some(taker_existing_position))?;

	let maker_price = maker.orders[maker_order_index].force_get_limit_price(
		Some(oracle_price),
		None,
		slot,
		market.amm.order_tick_size
	)?;
	let maker_side = maker.orders[maker_order_index].side;
	let maker_existing_position = maker.get_position(
		market.market_index
	)?.base_asset_amount;
	let maker_base_asset_amount = maker.orders[
		maker_order_index
	].get_base_asset_amount_unfilled(Some(maker_existing_position))?;

	let orders_cross = do_orders_cross(maker_side, maker_price, taker_price);

	if !orders_cross {
		msg!(
			"orders dont cross. maker price {} taker price {}",
			maker_price,
			taker_price
		);
		return Ok((0_u64, 0_u64, 0_u64));
	}

	let (base_asset_amount, _) = calculate_fill_for_matched_orders(
		maker_base_asset_amount,
		maker_price,
		taker_base_asset_amount,
		PERP_DECIMALS,
		maker_side
	)?;

	if base_asset_amount == 0 {
		return Ok((0_u64, 0_u64, 0_u64));
	}

	let sanitize_clamp_denominator = market.get_sanitize_clamp_denominator()?;
	amm::update_mark_twap_from_estimates(
		&mut market.amm,
		now,
		Some(maker_price),
		Some(taker_side),
		sanitize_clamp_denominator
	)?;

	let mut total_quote_asset_amount = 0_u64;
	let mut total_base_asset_amount = 0_u64;

	let (jit_base_asset_amount, amm_liquidity_split) =
		calculate_amm_jit_liquidity(
			market,
			taker_side,
			maker_price,
			valid_oracle_price,
			base_asset_amount,
			taker_base_asset_amount,
			maker_base_asset_amount,
			taker.orders[taker_order_index].has_limit_price(slot)?
		)?;

	if jit_base_asset_amount > 0 {
		let (base_asset_amount_filled_by_amm, quote_asset_amount_filled_by_amm) =
			fulfill_order_with_amm(
				taker,
				taker_stats,
				taker_order_index,
				market,
				oracle_map,
				reserve_price_before,
				now,
				slot,
				taker_key,
				filler_key,
				filler,
				filler_stats,
				&mut None,
				&mut None,
				&mut None,
				&mut None,
				fee_structure,
				taker_limit_price,
				Some(jit_base_asset_amount),
				Some(maker_price), // match the makers price
				amm_liquidity_split
			)?;

		total_base_asset_amount = base_asset_amount_filled_by_amm;
		total_quote_asset_amount = quote_asset_amount_filled_by_amm;
	}

	let taker_existing_position = taker.get_position(
		market.market_index
	)?.base_asset_amount;

	let taker_base_asset_amount = taker.orders[
		taker_order_index
	].get_base_asset_amount_unfilled(Some(taker_existing_position))?;

	let (base_asset_amount_fulfilled_by_maker, quote_asset_amount) =
		calculate_fill_for_matched_orders(
			maker_base_asset_amount,
			maker_price,
			taker_base_asset_amount,
			PERP_DECIMALS,
			maker_side
		)?;

	validate_fill_price(
		quote_asset_amount,
		base_asset_amount_fulfilled_by_maker,
		BASE_PRECISION_U64,
		taker_side,
		taker_price,
		true
	)?;

	validate_fill_price(
		quote_asset_amount,
		base_asset_amount_fulfilled_by_maker,
		BASE_PRECISION_U64,
		maker_side,
		maker_price,
		false
	)?;

	total_base_asset_amount = total_base_asset_amount.safe_add(
		base_asset_amount_fulfilled_by_maker
	)?;
	total_quote_asset_amount =
		total_quote_asset_amount.safe_add(quote_asset_amount)?;

	let maker_position_index = get_position_index(
		&maker.positions,
		maker.orders[maker_order_index].market_index
	)?;

	let maker_position_delta = get_position_delta_for_fill(
		base_asset_amount_fulfilled_by_maker,
		quote_asset_amount,
		maker.orders[maker_order_index].side
	)?;

	update_position_and_market(
		&mut maker.positions[maker_position_index],
		market,
		&maker_position_delta
	)?;

	// if maker is none, makes maker and taker authority was the same
	if let Some(maker_stats) = maker_stats {
		maker_stats.update_maker_volume_30d(quote_asset_amount, now)?;
	} else {
		taker_stats.update_maker_volume_30d(quote_asset_amount, now)?;
	}

	let taker_position_index = get_position_index(
		&taker.positions,
		taker.orders[taker_order_index].market_index
	)?;

	let taker_position_delta = get_position_delta_for_fill(
		base_asset_amount_fulfilled_by_maker,
		quote_asset_amount,
		taker.orders[taker_order_index].side
	)?;

	update_position_and_market(
		&mut taker.positions[taker_position_index],
		market,
		&taker_position_delta
	)?;

	taker_stats.update_taker_volume_30d(quote_asset_amount, now)?;

	let reward_referrer = can_reward_user_with_pnl(referrer, market.market_index);
	let reward_filler = can_reward_user_with_pnl(filler, market.market_index);

	let filler_multiplier = if reward_filler {
		calculate_filler_multiplier_for_matched_orders(
			maker_price,
			maker_side,
			oracle_price
		)?
	} else {
		0
	};

	let FillFees {
		user_fee: taker_fee,
		maker_rebate,
		fee_to_market,
		filler_reward,
		referrer_reward,
		referee_discount,
		..
	} = fees::calculate_fee_for_fulfillment_with_match(
		taker_stats,
		maker_stats,
		quote_asset_amount,
		fee_structure,
		taker.orders[taker_order_index].slot,
		slot,
		filler_multiplier,
		reward_referrer,
		referrer_stats,
		&MarketType::Synthetic,
		market.fee_adjustment
	)?;

	// Increment the markets house's total fee variables
	market.amm.total_fee = market.amm.total_fee.safe_add(fee_to_market.cast()?)?;
	market.amm.total_exchange_fee = market.amm.total_exchange_fee.safe_add(
		fee_to_market.cast()?
	)?;
	market.amm.total_fee_minus_distributions =
		market.amm.total_fee_minus_distributions.safe_add(fee_to_market.cast()?)?;

	controller::position::update_quote_asset_amount(
		&mut taker.positions[taker_position_index],
		market,
		-taker_fee.cast()?
	)?;

	taker_stats.increment_total_fees(taker_fee)?;
	taker_stats.increment_total_referee_discount(referee_discount)?;

	controller::position::update_quote_asset_amount(
		&mut maker.positions[maker_position_index],
		market,
		maker_rebate.cast()?
	)?;

	if let Some(maker_stats) = maker_stats {
		maker_stats.increment_total_rebate(maker_rebate)?;
	} else {
		taker_stats.increment_total_rebate(maker_rebate)?;
	}

	if let Some(filler) = filler {
		if filler_reward > 0 {
			let filler_position_index = get_position_index(
				&filler.positions,
				market.market_index
			).or_else(|_| {
				add_new_position(&mut filler.positions, market.market_index)
			})?;

			controller::position::update_quote_asset_amount(
				&mut filler.positions[filler_position_index],
				market,
				filler_reward.cast()?
			)?;

			filler_stats
				.as_mut()
				.safe_unwrap()?
				.update_filler_volume(quote_asset_amount, now)?;
		}
		filler.update_last_active_slot(slot);
	}

	if
		let (Some(referrer), Some(referrer_stats)) = (
			referrer.as_mut(),
			referrer_stats.as_mut(),
		)
	{
		if
			let Ok(referrer_position) = referrer.force_get_position_mut(
				market.market_index
			)
		{
			if referrer_reward > 0 {
				update_quote_asset_amount(
					referrer_position,
					market,
					referrer_reward.cast()?
				)?;
			}
			referrer_stats.increment_total_referrer_reward(referrer_reward, now)?;
		}
	}

	update_order_after_fill(
		&mut taker.orders[taker_order_index],
		base_asset_amount_fulfilled_by_maker,
		quote_asset_amount
	)?;

	decrease_open_bids_and_asks(
		&mut taker.positions[taker_position_index],
		&taker.orders[taker_order_index].side,
		base_asset_amount_fulfilled_by_maker
	)?;

	update_order_after_fill(
		&mut maker.orders[maker_order_index],
		base_asset_amount_fulfilled_by_maker,
		quote_asset_amount
	)?;

	decrease_open_bids_and_asks(
		&mut maker.positions[maker_position_index],
		&maker.orders[maker_order_index].side,
		base_asset_amount_fulfilled_by_maker
	)?;

	let fill_record_id = get_then_update_id!(market, next_fill_record_id);
	let order_action_explanation = if
		maker.orders[maker_order_index].is_jit_maker()
	{
		OrderActionExplanation::OrderFilledWithMatchJit
	} else {
		OrderActionExplanation::OrderFilledWithMatch
	};
	let order_action_record = get_order_action_record(
		now,
		OrderAction::Fill,
		order_action_explanation,
		market.market_index,
		Some(*filler_key),
		Some(fill_record_id),
		Some(filler_reward),
		Some(base_asset_amount_fulfilled_by_maker),
		Some(quote_asset_amount),
		Some(taker_fee),
		Some(maker_rebate),
		Some(referrer_reward),
		None,
		None,
		Some(*taker_key),
		Some(taker.orders[taker_order_index]),
		Some(*maker_key),
		Some(maker.orders[maker_order_index]),
		oracle_map.get_price_data(&market.amm.oracle)?.price
	)?;
	emit_stack::<_, { OrderActionRecord::SIZE }>(order_action_record)?;

	if taker.orders[taker_order_index].get_base_asset_amount_unfilled(None)? == 0 {
		taker.decrement_open_orders(taker.orders[taker_order_index].has_auction());
		taker.orders[taker_order_index] = Order::default();
		let market_position = &mut taker.positions[taker_position_index];
		market_position.open_orders -= 1;
	}

	if maker.orders[maker_order_index].get_base_asset_amount_unfilled(None)? == 0 {
		maker.decrement_open_orders(maker.orders[maker_order_index].has_auction());
		maker.orders[maker_order_index] = Order::default();
		let market_position = &mut maker.positions[maker_position_index];
		market_position.open_orders -= 1;
	}

	// TODO: custom token exchange - not sure where to place yet
	// 1. Send tokens from maker to taker
	controller::token::send_from_program_vault(
		&market.amm.token,
		maker_key,
		taker_key,
		&ctx.accounts.normal_signer,
		nonce,
		amount,
		&market.amm.token_mint
	);

	// 2. Send tokens from taker to maker
	controller::token::send_from_program_vault(
		&market.amm.token,
		taker_key,
		maker_key,
		&ctx.accounts.normal_signer,
		nonce,
		amount,
		&market.amm.token_mint
	);

	Ok((
		total_base_asset_amount,
		total_quote_asset_amount,
		base_asset_amount_fulfilled_by_maker,
	))
}

pub fn update_order_after_fill(
	order: &mut Order,
	base_asset_amount: u64,
	quote_asset_amount: u64
) -> NormalResult {
	order.base_asset_amount_filled =
		order.base_asset_amount_filled.safe_add(base_asset_amount)?;

	order.quote_asset_amount_filled =
		order.quote_asset_amount_filled.safe_add(quote_asset_amount)?;

	if order.get_base_asset_amount_unfilled(None)? == 0 {
		order.status = OrderStatus::Filled;
	}

	Ok(())
}

#[allow(clippy::type_complexity)]
fn get_taker_and_maker_for_order_record(
	user_key: &Pubkey,
	user_order: &Order
) -> (Option<Pubkey>, Option<Order>, Option<Pubkey>, Option<Order>) {
	if user_order.post_only {
		(None, None, Some(*user_key), Some(*user_order))
	} else {
		(Some(*user_key), Some(*user_order), None, None)
	}
}

pub fn trigger_order(
	order_id: u32,
	state: &State,
	user: &AccountLoader<User>,
	market_map: &MarketMap,
	oracle_map: &mut OracleMap,
	filler: &AccountLoader<User>,
	clock: &Clock
) -> NormalResult {
	let now = clock.unix_timestamp;
	let slot = clock.slot;

	let filler_key = filler.key();
	let user_key = user.key();
	let user = &mut load_mut!(user)?;

	let order_index = user.orders
		.iter()
		.position(|order| order.order_id == order_id)
		.ok_or_else(print_error!(ErrorCode::OrderDoesNotExist))?;

	let (order_status, market_index, market_type) = get_struct_values!(
		user.orders[order_index],
		status,
		market_index,
		market_type
	);

	validate!(
		order_status == OrderStatus::Open,
		ErrorCode::OrderNotOpen,
		"Order not open"
	)?;

	validate!(
		user.orders[order_index].must_be_triggered(),
		ErrorCode::OrderNotTriggerable,
		"Order is not triggerable"
	)?;

	validate!(
		!user.orders[order_index].triggered(),
		ErrorCode::OrderNotTriggerable,
		"Order is already triggered"
	)?;

	validate!(
		market_type == MarketType::Synthetic,
		ErrorCode::InvalidOrderMarketType,
		"Order must be a synthetic order"
	)?;

	let mut market = market_map.get_ref_mut(&market_index)?;
	let (oracle_price_data, oracle_validity) =
		oracle_map.get_price_data_and_validity(
			MarketType::Synthetic,
			market.market_index,
			&market.amm.oracle,
			market.amm.historical_oracle_data.last_oracle_price_twap,
			market.get_max_confidence_interval_multiplier()?
		)?;

	let is_oracle_valid = is_oracle_valid_for_action(
		oracle_validity,
		Some(NormalAction::TriggerOrder)
	)?;

	validate!(is_oracle_valid, ErrorCode::InvalidOracle)?;

	let oracle_price = oracle_price_data.price;

	let oracle_too_divergent_with_twap_5min =
		is_oracle_too_divergent_with_twap_5min(
			oracle_price_data.price,
			market.amm.historical_oracle_data.last_oracle_price_twap_5min,
			state.oracle_guard_rails.max_oracle_twap_5min_percent_divergence().cast()?
		)?;

	validate!(
		!oracle_too_divergent_with_twap_5min,
		ErrorCode::OrderBreachesOraclePriceLimits,
		"oracle price vs twap too divergent"
	)?;

	let can_trigger = order_satisfies_trigger_condition(
		&user.orders[order_index],
		oracle_price.unsigned_abs().cast()?
	)?;
	validate!(can_trigger, ErrorCode::OrderDidNotSatisfyTriggerCondition)?;

	let (_, worst_case_liability_value_before) = user
		.get_position(market_index)?
		.worst_case_liability_value(oracle_price, market.contract_type)?;

	{
		update_trigger_order_params(
			&mut user.orders[order_index],
			oracle_price_data,
			slot,
			30,
			Some(&market)
		)?;

		if user.orders[order_index].has_auction() {
			user.increment_open_auctions();
		}

		let side = user.orders[order_index].side;
		let base_asset_amount = user.orders[order_index].base_asset_amount;

		let user_position = user.get_position_mut(market_index)?;
		increase_open_bids_and_asks(user_position, &side, base_asset_amount)?;
	}

	let is_filler_taker = user_key == filler_key;
	let mut filler = if !is_filler_taker {
		Some(load_mut!(filler)?)
	} else {
		None
	};

	let filler_reward = pay_keeper_flat_reward(
		user,
		filler.as_deref_mut(),
		&mut market,
		state.fee_structure.flat_filler_fee,
		slot
	)?;

	let order_action_record = get_order_action_record(
		now,
		OrderAction::Trigger,
		OrderActionExplanation::None,
		market_index,
		Some(filler_key),
		None,
		Some(filler_reward),
		None,
		None,
		Some(filler_reward),
		None,
		None,
		None,
		None,
		Some(user_key),
		Some(user.orders[order_index]),
		None,
		None,
		oracle_price
	)?;
	emit!(order_action_record);

	drop(market);

	user.update_last_active_slot(slot);

	Ok(())
}

fn update_trigger_order_params(
	order: &mut Order,
	oracle_price_data: &OraclePriceData,
	slot: u64,
	min_auction_duration: u8,
	market: Option<&Market>
) -> NormalResult {
	order.trigger_condition = match order.trigger_condition {
		OrderTriggerCondition::Above => OrderTriggerCondition::TriggeredAbove,
		OrderTriggerCondition::Below => OrderTriggerCondition::TriggeredBelow,
		_ => {
			return Err(print_error!(ErrorCode::InvalidTriggerOrderCondition)());
		}
	};

	order.slot = slot;

	let (auction_duration, auction_start_price, auction_end_price) =
		calculate_auction_params_for_trigger_order(
			order,
			oracle_price_data,
			min_auction_duration,
			market
		)?;

	msg!(
		"new auction duration {} start price {} end price {}",
		auction_duration,
		auction_start_price,
		auction_end_price
	);

	order.auction_duration = auction_duration;
	order.auction_start_price = auction_start_price;
	order.auction_end_price = auction_end_price;

	Ok(())
}

pub fn force_cancel_orders(
	state: &State,
	user_account_loader: &AccountLoader<User>,
	market_map: &MarketMap,
	oracle_map: &mut OracleMap,
	filler: &AccountLoader<User>,
	clock: &Clock
) -> NormalResult {
	let now = clock.unix_timestamp;
	let slot = clock.slot;

	let filler_key = filler.key();
	let user_key = user_account_loader.key();
	let user = &mut load_mut!(user_account_loader)?;
	let filler = &mut load_mut!(filler)?;

	let mut total_fee = 0_u64;

	for order_index in 0..user.orders.len() {
		if user.orders[order_index].status != OrderStatus::Open {
			continue;
		}

		let market_index = user.orders[order_index].market_index;
		let market_type = user.orders[order_index].market_type;

		let fee = {
			let base_asset_amount = user.get_position(
				market_index
			)?.base_asset_amount;
			let is_position_reducing = is_order_position_reducing(
				&user.orders[order_index].side,
				user.orders[order_index].get_base_asset_amount_unfilled(
					Some(base_asset_amount)
				)?,
				base_asset_amount
			)?;
			if is_position_reducing {
				continue;
			}

			state.fee_structure.flat_filler_fee
		};

		total_fee = total_fee.safe_add(fee)?;

		cancel_order(
			order_index,
			user,
			&user_key,
			market_map,
			oracle_map,
			now,
			slot,
			OrderActionExplanation::InsufficientFunds,
			Some(&filler_key),
			fee,
			false
		)?;
	}

	pay_keeper_flat_reward_for_spot(
		user,
		Some(filler),
		market_map.get_quote_spot_market_mut()?.deref_mut(),
		total_fee,
		slot
	)?;

	user.update_last_active_slot(slot);

	Ok(())
}

pub fn can_reward_user_with_pnl(
	user: &mut Option<&mut User>,
	market_index: u16
) -> bool {
	match user.as_mut() {
		Some(user) => user.force_get_position_mut(market_index).is_ok(),
		None => false,
	}
}


pub fn pay_keeper_flat_reward(
	user: &mut User,
	filler: Option<&mut User>,
	market: &mut Market,
	filler_reward: u64,
	slot: u64
) -> NormalResult<u64> {
	let filler_reward = if let Some(filler) = filler {
		let user_position = user.get_position_mut(market.market_index)?;
		controller::position::update_quote_asset_amount(
			user_position,
			market,
			-filler_reward.cast()?
		)?;

		filler.update_last_active_slot(slot);
		// Dont throw error if filler doesnt have position available
		let filler_position = match
			filler.force_get_position_mut(market.market_index)
		{
			Ok(position) => position,
			Err(_) => {
				return Ok(0);
			}
		};
		controller::position::update_quote_asset_amount(
			filler_position,
			market,
			filler_reward.cast()?
		)?;

		filler_reward
	} else {
		0
	};

	Ok(filler_reward)
}

pub fn expire_orders(
	user: &mut User,
	user_key: &Pubkey,
	market_map: &MarketMap,
	oracle_map: &mut OracleMap,
	now: i64,
	slot: u64
) -> NormalResult {
	for order_index in 0..user.orders.len() {
		if !should_expire_order(user, order_index, now)? {
			continue;
		}

		cancel_order(
			order_index,
			user,
			user_key,
			market_map,
			oracle_map,
			now,
			slot,
			OrderActionExplanation::OrderExpired,
			None,
			0,
			false
		)?;
	}

	Ok(())
}
