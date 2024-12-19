use anchor_lang::prelude::*;

pub fn create_schedule(
	state: &State,
	user: &mut User,
	user_key: Pubkey,
	perp_market_map: &PerpMarketMap,
	spot_market_map: &SpotMarketMap,
	oracle_map: &mut OracleMap,
	clock: &Clock,
	mut params: OrderParams,
	mut options: PlaceOrderOptions
) -> DriftResult {
	let now = clock.unix_timestamp;
	let slot: u64 = clock.slot;

	if !options.is_liquidation() {
		validate_user_not_being_liquidated(
			user,
			perp_market_map,
			spot_market_map,
			oracle_map,
			state.liquidation_margin_buffer_ratio
		)?;
	}

	validate!(!user.is_bankrupt(), ErrorCode::UserBankrupt)?;

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
	let market = &perp_market_map.get_ref(&market_index)?;
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
		&user.perp_positions,
		market_index
	).or_else(|_| add_new_position(&mut user.perp_positions, market_index))?;

	// Increment open orders for existing position
	let (existing_position_direction, order_base_asset_amount) = {
		validate!(
			params.base_asset_amount >= market.amm.order_step_size,
			ErrorCode::OrderAmountTooSmall,
			"params.base_asset_amount={} cannot be below market.amm.order_step_size={}",
			params.base_asset_amount,
			market.amm.order_step_size
		)?;

		let base_asset_amount = if params.base_asset_amount == u64::MAX {
			calculate_max_perp_order_size(
				user,
				position_index,
				params.market_index,
				params.direction,
				perp_market_map,
				spot_market_map,
				oracle_map
			)?
		} else {
			standardize_base_asset_amount(
				params.base_asset_amount,
				market.amm.order_step_size
			)?
		};

		let market_position = &user.perp_positions[position_index];
		let existing_position_direction = if market_position.base_asset_amount >= 0 {
			PositionDirection::Long
		} else {
			PositionDirection::Short
		};
		(existing_position_direction, base_asset_amount)
	};

	let oracle_price_data = oracle_map.get_price_data(&market.amm.oracle)?;

	validate!(
		params.market_type == MarketType::Perp,
		ErrorCode::InvalidOrderMarketType,
		"must be perp order"
	)?;

	let new_order = Schedule {
		status: OrderStatus::Open,
		order_type: params.order_type,
		market_type: params.market_type,
		slot: options.get_order_slot(slot),
		order_id: get_then_update_id!(user, next_order_id),
		user_order_id: params.user_order_id,
		market_index: params.market_index,
		price: get_price_for_perp_order(
			params.price,
			params.direction,
			params.post_only,
			&market.amm
		)?,
		existing_position_direction,
		base_asset_amount: order_base_asset_amount,
		base_asset_amount_filled: 0,
		quote_asset_amount_filled: 0,
		direction: params.direction,
		reduce_only: params.reduce_only || force_reduce_only,
		trigger_price: standardize_price(
			params.trigger_price.unwrap_or(0),
			market.amm.order_tick_size,
			params.direction
		)?,
		trigger_condition: params.trigger_condition,
		post_only: params.post_only != PostOnlyParam::None,
		oracle_price_offset: params.oracle_price_offset.unwrap_or(0),
		immediate_or_cancel: params.immediate_or_cancel,
		auction_start_price,
		auction_end_price,
		auction_duration,
		max_ts,
		padding: [0; 3],
	};

	// validate schedule?

	user.increment_open_orders(new_order.has_auction());
	user.orders[new_order_index] = new_order;
	user.perp_positions[position_index].open_orders += 1;
	if !new_order.must_be_triggered() {
		increase_open_bids_and_asks(
			&mut user.perp_positions[position_index],
			&params.direction,
			order_base_asset_amount
		)?;
	}

	let order_record = OrderRecord {
		ts: now,
		user: user_key,
		order: user.orders[new_order_index],
	};
	emit_stack::<_, { OrderRecord::SIZE }>(order_record)?;

	user.update_last_active_slot(slot);

	Ok(())
}

pub fn modify_schedule(
	order_id: ModifyOrderId,
	modify_order_params: ModifyOrderParams,
	user_loader: &AccountLoader<User>,
	state: &State,
	perp_market_map: &PerpMarketMap,
	spot_market_map: &SpotMarketMap,
	oracle_map: &mut OracleMap,
	clock: &Clock
) -> DriftResult {
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
		perp_market_map,
		spot_market_map,
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

	if order_params.market_type == MarketType::Perp {
		place_perp_order(
			state,
			&mut user,
			user_key,
			perp_market_map,
			spot_market_map,
			oracle_map,
			clock,
			order_params,
			PlaceOrderOptions::default()
		)?;
	} else {
		place_spot_order(
			state,
			&mut user,
			user_key,
			perp_market_map,
			spot_market_map,
			oracle_map,
			clock,
			order_params,
			PlaceOrderOptions::default()
		)?;
	}

	Ok(())
}

fn merge_modify_schedule_params_with_existing_schedule(
	existing_order: &Order,
	modify_schedule_params: &ModifyScheduleParams
) -> DriftResult<OrderParams> {
	let market_type = existing_order.market_type;
	let direction = modify_schedule_params.direction.unwrap_or(
		existing_order.direction
	);
	let base_asset_amount = modify_schedule_params.base_asset_amount.unwrap_or(
		existing_order.get_base_asset_amount_unfilled(None)?
	);
	let price = modify_schedule_params.price.unwrap_or(existing_order.price);
	let market_index = existing_order.market_index;

	let trigger_price = modify_schedule_params.trigger_price.or(
		Some(existing_order.trigger_price)
	);

	Ok(OrderParams {
		order_type,
		market_type,
		direction,
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
		oracle_price_offset,
		auction_duration,
		auction_start_price,
		auction_end_price,
	})
}

pub fn execute_schedule_order(
	order_id: u32,
	state: &State,
	user: &AccountLoader<User>,
	user_stats: &AccountLoader<UserStats>,
	spot_market_map: &SpotMarketMap,
	perp_market_map: &PerpMarketMap,
	oracle_map: &mut OracleMap,
	filler: &AccountLoader<User>,
	clock: &Clock
) -> DriftResult<(u64, u64)> {
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

	let (order_status, market_index, order_market_type, order_direction) =
		get_struct_values!(
			user.orders[order_index],
			status,
			market_index,
			market_type,
			direction
		);

	validate!(
		order_market_type == MarketType::Perp,
		ErrorCode::InvalidOrderMarketType,
		"must be perp order"
	)?;

	// settle lp position so its tradeable
	let mut market = perp_market_map.get_ref_mut(&market_index)?;
	controller::lp::settle_funding_payment_then_lp(
		user,
		&user_key,
		&mut market,
		now
	)?;

	validate!(
		matches!(market.status, MarketStatus::Active | MarketStatus::ReduceOnly),
		ErrorCode::MarketFillOrderPaused,
		"Market not active"
	)?;

	validate!(
		!market.is_operation_paused(PerpOperation::Fill),
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

	if user.is_bankrupt() {
		msg!("user is bankrupt");
		return Ok((0, 0));
	}

	// ...
}
