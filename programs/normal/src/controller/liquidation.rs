use std::ops::{ Deref, DerefMut };

use anchor_lang::prelude::*;
use solana_program::msg;

use crate::controller::amm::get_fee_pool_tokens;
use crate::controller::funding::settle_funding_payment;
use crate::controller::lp::burn_lp_shares;
use crate::controller::orders;
use crate::controller::orders::{
	cancel_order,
	fill_perp_order,
	place_perp_order,
};
use crate::controller::position::{
	get_position_index,
	update_position_and_market,
	update_quote_asset_amount,
	update_quote_asset_and_break_even_amount,
	PositionDirection,
};
use crate::controller::repeg::update_amm_and_check_validity;
use crate::controller::spot_balance::{
	update_revenue_pool_balances,
	update_spot_balances,
	update_spot_market_and_check_validity,
	update_spot_market_cumulative_interest,
};
use crate::controller::spot_position::update_spot_balances_and_cumulative_deposits;
use crate::error::{ DriftResult, ErrorCode };
use crate::math::bankruptcy::is_user_bankrupt;
use crate::math::casting::Cast;
use crate::math::constants::{
	LIQUIDATION_FEE_PRECISION_U128,
	LIQUIDATION_PCT_PRECISION,
	QUOTE_PRECISION,
	QUOTE_PRECISION_I128,
	QUOTE_PRECISION_U64,
	QUOTE_SPOT_MARKET_INDEX,
	SPOT_WEIGHT_PRECISION,
};
use crate::math::liquidation::{
	calculate_asset_transfer_for_liability_transfer,
	calculate_base_asset_amount_to_cover_margin_shortage,
	calculate_cumulative_deposit_interest_delta_to_resolve_bankruptcy,
	calculate_funding_rate_deltas_to_resolve_bankruptcy,
	calculate_liability_transfer_implied_by_asset_amount,
	calculate_liability_transfer_to_cover_margin_shortage,
	calculate_liquidation_multiplier,
	calculate_max_pct_to_liquidate,
	calculate_perp_if_fee,
	calculate_spot_if_fee,
	get_liquidation_fee,
	get_liquidation_order_params,
	validate_transfer_satisfies_limit_price,
	LiquidationMultiplierType,
};
use crate::math::margin::{
	calculate_margin_requirement_and_total_collateral_and_liability_info,
	calculate_user_safest_position_tiers,
	meets_initial_margin_requirement,
	MarginRequirementType,
};
use crate::math::oracle::DriftAction;
use crate::math::orders::{
	get_position_delta_for_fill,
	is_multiple_of_step_size,
	is_oracle_too_divergent_with_twap_5min,
	standardize_base_asset_amount,
	standardize_base_asset_amount_ceil,
};
use crate::math::position::calculate_base_asset_value_with_oracle_price;
use crate::math::safe_math::SafeMath;

use crate::math::spot_balance::get_token_value;
use crate::state::events::{
	emit_stack,
	LPAction,
	LPRecord,
	LiquidateBorrowForPerpPnlRecord,
	LiquidatePerpPnlForDepositRecord,
	LiquidatePerpRecord,
	LiquidateSpotRecord,
	LiquidationRecord,
	LiquidationType,
	OrderAction,
	OrderActionExplanation,
	OrderActionRecord,
	OrderRecord,
	PerpBankruptcyRecord,
	SpotBankruptcyRecord,
};
use crate::state::fill_mode::FillMode;
use crate::state::margin_calculation::{
	MarginCalculation,
	MarginContext,
	MarketIdentifier,
};
use crate::state::oracle_map::OracleMap;
use crate::state::order_params::PlaceOrderOptions;
use crate::state::paused_operations::{ PerpOperation, SpotOperation };
use crate::state::perp_market::MarketStatus;
use crate::state::perp_market_map::PerpMarketMap;
use crate::state::spot_market::SpotBalanceType;
use crate::state::spot_market_map::SpotMarketMap;
use crate::state::state::State;
use crate::state::traits::Size;
use crate::state::user::{
	MarketType,
	Order,
	OrderStatus,
	OrderType,
	User,
	UserStats,
};
use crate::state::user_map::{ UserMap, UserStatsMap };
use crate::state::vault::Vault;
use crate::validate;
use crate::{ get_then_update_id, load_mut };

pub fn liquidate_vault(
	market_index: u16,
	liquidator_max_base_asset_amount: u64,
	limit_price: Option<u64>,
	user: &mut User,
	user_key: &Pubkey,
	user_stats: &mut UserStats,
	liquidator: &mut User,
	liquidator_key: &Pubkey,
	liquidator_stats: &mut UserStats,
	perp_market_map: &PerpMarketMap,
	spot_market_map: &SpotMarketMap,
	oracle_map: &mut OracleMap,
	slot: u64,
	now: i64,
	state: &State
) -> DriftResult {
	let liquidation_margin_buffer_ratio = state.liquidation_margin_buffer_ratio;
	let initial_pct_to_liquidate = state.initial_pct_to_liquidate as u128;
	let liquidation_duration = state.liquidation_duration as u128;

	validate!(!user.is_bankrupt(), ErrorCode::UserBankrupt, "user bankrupt")?;

	validate!(
		!liquidator.is_bankrupt(),
		ErrorCode::UserBankrupt,
		"liquidator bankrupt"
	)?;

	let market = perp_market_map.get_ref(&market_index)?;

	validate!(
		!market.is_operation_paused(PerpOperation::Liquidation),
		ErrorCode::InvalidLiquidation,
		"Liquidation operation is paused for market {}",
		market_index
	)?;

	drop(market);

	settle_funding_payment(
		user,
		user_key,
		perp_market_map.get_ref_mut(&market_index)?.deref_mut(),
		now
	)?;

	settle_funding_payment(
		liquidator,
		liquidator_key,
		perp_market_map.get_ref_mut(&market_index)?.deref_mut(),
		now
	)?;

	let margin_calculation =
		calculate_margin_requirement_and_total_collateral_and_liability_info(
			user,
			perp_market_map,
			spot_market_map,
			oracle_map,
			MarginContext::liquidation(
				liquidation_margin_buffer_ratio
			).track_market_margin_requirement(MarketIdentifier::perp(market_index))?
		)?;

	if
		!user.is_being_liquidated() &&
		margin_calculation.meets_margin_requirement()
	{
		msg!("margin calculation: {:?}", margin_calculation);
		return Err(ErrorCode::SufficientCollateral);
	} else if
		user.is_being_liquidated() &&
		margin_calculation.can_exit_liquidation()?
	{
		user.exit_liquidation();
		return Ok(());
	}

	user.get_perp_position(market_index).map_err(|e| {
		msg!("User does not have a position for perp market {}", market_index);
		e
	})?;

	liquidator.force_get_perp_position_mut(market_index).map_err(|e| {
		msg!("Liquidator has no available positions to take on perp position in market {}", market_index);
		e
	})?;

	let liquidation_id = user.enter_liquidation(slot)?;
	let mut margin_freed = 0_u64;

	let position_index = get_position_index(&user.perp_positions, market_index)?;
	validate!(
		user.perp_positions[position_index].is_open_position() ||
			user.perp_positions[position_index].has_open_order() ||
			user.perp_positions[position_index].is_lp(),
		ErrorCode::PositionDoesntHaveOpenPositionOrOrders
	)?;

	let canceled_order_ids = orders::cancel_orders(
		user,
		user_key,
		Some(liquidator_key),
		perp_market_map,
		spot_market_map,
		oracle_map,
		now,
		slot,
		OrderActionExplanation::Liquidation,
		None,
		None,
		None
	)?;

	let mut market = perp_market_map.get_ref_mut(&market_index)?;
	let oracle_price_data = oracle_map.get_price_data(&market.amm.oracle)?;

	update_amm_and_check_validity(
		&mut market,
		oracle_price_data,
		state,
		now,
		slot,
		Some(DriftAction::Liquidate)
	)?;

	let oracle_price = if market.status == MarketStatus::Settlement {
		market.expiry_price
	} else {
		oracle_price_data.price
	};

	drop(market);

	// burning lp shares = removing open bids/asks
	let lp_shares = user.perp_positions[position_index].lp_shares;
	if lp_shares > 0 {
		let (position_delta, pnl) = burn_lp_shares(
			&mut user.perp_positions[position_index],
			perp_market_map.get_ref_mut(&market_index)?.deref_mut(),
			lp_shares,
			oracle_price
		)?;

		// emit LP record for shares removed
		emit_stack::<_, { LPRecord::SIZE }>(LPRecord {
			ts: now,
			action: LPAction::RemoveLiquidity,
			user: *user_key,
			n_shares: lp_shares,
			market_index,
			delta_base_asset_amount: position_delta.base_asset_amount,
			delta_quote_asset_amount: position_delta.quote_asset_amount,
			pnl,
		})?;
	}

	// check if user exited liquidation territory
	let intermediate_margin_calculation = if
		!canceled_order_ids.is_empty() ||
		lp_shares > 0
	{
		let intermediate_margin_calculation =
			calculate_margin_requirement_and_total_collateral_and_liability_info(
				user,
				perp_market_map,
				spot_market_map,
				oracle_map,
				MarginContext::liquidation(
					liquidation_margin_buffer_ratio
				).track_market_margin_requirement(MarketIdentifier::perp(market_index))?
			)?;

		let initial_margin_shortage = margin_calculation.margin_shortage()?;
		let new_margin_shortage =
			intermediate_margin_calculation.margin_shortage()?;

		margin_freed = initial_margin_shortage
			.saturating_sub(new_margin_shortage)
			.cast::<u64>()?;
		user.increment_margin_freed(margin_freed)?;

		if intermediate_margin_calculation.can_exit_liquidation()? {
			emit!(LiquidationRecord {
				ts: now,
				liquidation_id,
				liquidation_type: LiquidationType::LiquidatePerp,
				user: *user_key,
				liquidator: *liquidator_key,
				margin_requirement: margin_calculation.margin_requirement,
				total_collateral: margin_calculation.total_collateral,
				bankrupt: user.is_bankrupt(),
				canceled_order_ids,
				margin_freed,
				liquidate_perp: LiquidatePerpRecord {
					market_index,
					oracle_price,
					lp_shares,
					..LiquidatePerpRecord::default()
				},
				..LiquidationRecord::default()
			});

			user.exit_liquidation();
			return Ok(());
		}

		intermediate_margin_calculation
	} else {
		margin_calculation
	};

	if user.perp_positions[position_index].base_asset_amount == 0 {
		msg!("User has no base asset amount");
		return Ok(());
	}

	let liquidator_max_base_asset_amount = standardize_base_asset_amount(
		liquidator_max_base_asset_amount,
		perp_market_map.get_ref(&market_index)?.amm.order_step_size
	)?;

	validate!(
		liquidator_max_base_asset_amount != 0,
		ErrorCode::InvalidBaseAssetAmountForLiquidatePerp,
		"liquidator_max_base_asset_amount must be greater or equal to the step size"
	)?;

	let oracle_price_too_divergent = is_oracle_too_divergent_with_twap_5min(
		oracle_price,
		perp_market_map.get_ref(
			&market_index
		)?.amm.historical_oracle_data.last_oracle_price_twap_5min,
		state.oracle_guard_rails.max_oracle_twap_5min_percent_divergence().cast()?
	)?;

	validate!(!oracle_price_too_divergent, ErrorCode::PriceBandsBreached)?;

	let user_base_asset_amount =
		user.perp_positions[position_index].base_asset_amount.unsigned_abs();

	let margin_ratio = perp_market_map
		.get_ref(&market_index)?
		.get_margin_ratio(
			user_base_asset_amount.cast()?,
			MarginRequirementType::Maintenance
		)?;

	let margin_ratio_with_buffer = margin_ratio.safe_add(
		liquidation_margin_buffer_ratio
	)?;

	let margin_shortage = intermediate_margin_calculation.margin_shortage()?;

	let market = perp_market_map.get_ref(&market_index)?;
	let quote_spot_market = spot_market_map.get_ref(
		&market.quote_spot_market_index
	)?;
	let quote_oracle_price = oracle_map.get_price_data(
		&quote_spot_market.oracle
	)?.price;
	let liquidator_fee = market.liquidator_fee;
	let if_liquidation_fee = calculate_perp_if_fee(
		intermediate_margin_calculation.tracked_market_margin_shortage(
			margin_shortage
		)?,
		user_base_asset_amount,
		margin_ratio_with_buffer,
		liquidator_fee,
		oracle_price,
		quote_oracle_price,
		market.if_liquidation_fee
	)?;
	let base_asset_amount_to_cover_margin_shortage =
		standardize_base_asset_amount_ceil(
			calculate_base_asset_amount_to_cover_margin_shortage(
				margin_shortage,
				margin_ratio_with_buffer,
				liquidator_fee,
				if_liquidation_fee,
				oracle_price,
				quote_oracle_price
			)?,
			market.amm.order_step_size
		)?;
	drop(market);
	drop(quote_spot_market);

	let max_pct_allowed = calculate_max_pct_to_liquidate(
		user,
		margin_shortage,
		slot,
		initial_pct_to_liquidate,
		liquidation_duration
	)?;
	let max_base_asset_amount_allowed_to_be_transferred =
		base_asset_amount_to_cover_margin_shortage
			.cast::<u128>()?
			.saturating_mul(max_pct_allowed)
			.safe_div(LIQUIDATION_PCT_PRECISION)?
			.cast::<u64>()?;

	if max_base_asset_amount_allowed_to_be_transferred == 0 {
		msg!("max_base_asset_amount_allowed_to_be_transferred == 0");
		return Ok(());
	}

	let base_asset_value = calculate_base_asset_value_with_oracle_price(
		user_base_asset_amount.cast()?,
		oracle_price
	)?.cast::<u64>()?;

	// if position is less than $50, liquidator can liq all of it
	let min_base_asset_amount = if base_asset_value > 50 * QUOTE_PRECISION_U64 {
		0_u64
	} else {
		user_base_asset_amount
	};

	let base_asset_amount = user_base_asset_amount
		.min(liquidator_max_base_asset_amount)
		.min(
			max_base_asset_amount_allowed_to_be_transferred.max(min_base_asset_amount)
		);
	let base_asset_amount = standardize_base_asset_amount_ceil(
		base_asset_amount,
		perp_market_map.get_ref(&market_index)?.amm.order_step_size
	)?;

	// Make sure liquidator enters at better than limit price
	if let Some(limit_price) = limit_price {
		match user.perp_positions[position_index].get_direction() {
			PositionDirection::Long =>
				validate!(
					oracle_price <= limit_price.cast()?,
					ErrorCode::LiquidationDoesntSatisfyLimitPrice,
					"limit price ({}) > oracle price ({})",
					limit_price,
					oracle_price
				)?,
			PositionDirection::Short =>
				validate!(
					oracle_price >= limit_price.cast()?,
					ErrorCode::LiquidationDoesntSatisfyLimitPrice,
					"limit price ({}) < oracle price ({})",
					limit_price,
					oracle_price
				)?,
		}
	}

	let base_asset_value = calculate_base_asset_value_with_oracle_price(
		base_asset_amount.cast()?,
		oracle_price
	)?.cast::<u64>()?;

	let liquidator_fee = -base_asset_value
		.cast::<u128>()?
		.safe_mul(liquidator_fee.cast()?)?
		.safe_div(LIQUIDATION_FEE_PRECISION_U128)?
		.cast::<i64>()?;

	let if_fee = -base_asset_value
		.cast::<u128>()?
		.safe_mul(if_liquidation_fee.cast()?)?
		.safe_div(LIQUIDATION_FEE_PRECISION_U128)?
		.cast::<i64>()?;

	user_stats.update_taker_volume_30d(
		perp_market_map.get_ref(&market_index)?.fuel_boost_taker,
		base_asset_value,
		now
	)?;
	liquidator_stats.update_maker_volume_30d(
		perp_market_map.get_ref(&market_index)?.fuel_boost_maker,
		base_asset_value,
		now
	)?;

	let user_position_delta = get_position_delta_for_fill(
		base_asset_amount,
		base_asset_value,
		user.perp_positions[position_index].get_direction_to_close()
	)?;

	let liquidator_position_delta = get_position_delta_for_fill(
		base_asset_amount,
		base_asset_value,
		user.perp_positions[position_index].get_direction()
	)?;

	let (
		user_existing_position_direction,
		user_position_direction_to_close,
		liquidator_existing_position_direction,
	) = {
		let mut market = perp_market_map.get_ref_mut(&market_index)?;

		let user_position = user.get_perp_position_mut(market_index)?;
		let user_existing_position_direction = user_position.get_direction();
		let user_position_direction_to_close =
			user_position.get_direction_to_close();
		update_position_and_market(
			user_position,
			&mut market,
			&user_position_delta
		)?;
		update_quote_asset_and_break_even_amount(
			user_position,
			&mut market,
			liquidator_fee
		)?;
		update_quote_asset_and_break_even_amount(
			user_position,
			&mut market,
			if_fee
		)?;

		validate!(
			is_multiple_of_step_size(
				user_position.base_asset_amount.unsigned_abs(),
				market.amm.order_step_size
			)?,
			ErrorCode::InvalidPerpPosition,
			"base asset amount {} step size {}",
			user_position.base_asset_amount,
			market.amm.order_step_size
		)?;

		let liquidator_position =
			liquidator.force_get_perp_position_mut(market_index)?;
		let liquidator_existing_position_direction =
			liquidator_position.get_direction();
		update_position_and_market(
			liquidator_position,
			&mut market,
			&liquidator_position_delta
		)?;
		update_quote_asset_and_break_even_amount(
			liquidator_position,
			&mut market,
			-liquidator_fee
		)?;

		validate!(
			is_multiple_of_step_size(
				liquidator_position.base_asset_amount.unsigned_abs(),
				market.amm.order_step_size
			)?,
			ErrorCode::InvalidPerpPosition,
			"base asset amount {} step size {}",
			liquidator_position.base_asset_amount,
			market.amm.order_step_size
		)?;

		market.amm.total_liquidation_fee =
			market.amm.total_liquidation_fee.safe_add(if_fee.unsigned_abs().cast()?)?;

		(
			user_existing_position_direction,
			user_position_direction_to_close,
			liquidator_existing_position_direction,
		)
	};

	let (margin_freed_for_perp_position, _) = calculate_margin_freed(
		user,
		perp_market_map,
		spot_market_map,
		oracle_map,
		liquidation_margin_buffer_ratio,
		margin_shortage
	)?;
	margin_freed = margin_freed.safe_add(margin_freed_for_perp_position)?;
	user.increment_margin_freed(margin_freed_for_perp_position)?;

	if base_asset_amount >= base_asset_amount_to_cover_margin_shortage {
		user.exit_liquidation();
	} else if is_user_bankrupt(user) {
		user.enter_bankruptcy();
	}

	let liquidator_meets_initial_margin_requirement =
		meets_initial_margin_requirement(
			liquidator,
			perp_market_map,
			spot_market_map,
			oracle_map
		)?;

	validate!(
		liquidator_meets_initial_margin_requirement,
		ErrorCode::InsufficientCollateral,
		"Liquidator doesnt have enough collateral to take over perp position"
	)?;

	// get ids for order fills
	let user_order_id = get_then_update_id!(user, next_order_id);
	let liquidator_order_id = get_then_update_id!(liquidator, next_order_id);
	let fill_record_id = {
		let mut market = perp_market_map.get_ref_mut(&market_index)?;
		get_then_update_id!(market, next_fill_record_id)
	};

	let user_order = Order {
		slot,
		base_asset_amount,
		order_id: user_order_id,
		market_index,
		status: OrderStatus::Open,
		order_type: OrderType::Market,
		market_type: MarketType::Perp,
		direction: user_position_direction_to_close,
		existing_position_direction: user_existing_position_direction,
		..Order::default()
	};

	emit!(OrderRecord {
		ts: now,
		user: *user_key,
		order: user_order,
	});

	let liquidator_order = Order {
		slot,
		price: if let Some(price) = limit_price {
			price
		} else {
			0
		},
		base_asset_amount,
		order_id: liquidator_order_id,
		market_index,
		status: OrderStatus::Open,
		order_type: if limit_price.is_some() {
			OrderType::Limit
		} else {
			OrderType::Market
		},
		market_type: MarketType::Perp,
		direction: user_existing_position_direction,
		existing_position_direction: liquidator_existing_position_direction,
		..Order::default()
	};

	emit!(OrderRecord {
		ts: now,
		user: *liquidator_key,
		order: liquidator_order,
	});

	let fill_record = OrderActionRecord {
		ts: now,
		action: OrderAction::Fill,
		action_explanation: OrderActionExplanation::Liquidation,
		market_index,
		market_type: MarketType::Perp,
		filler: None,
		filler_reward: None,
		fill_record_id: Some(fill_record_id),
		base_asset_amount_filled: Some(base_asset_amount),
		quote_asset_amount_filled: Some(base_asset_value),
		taker_fee: Some(
			liquidator_fee.unsigned_abs().safe_add(if_fee.unsigned_abs())?
		),
		maker_fee: Some(liquidator_fee),
		referrer_reward: None,
		quote_asset_amount_surplus: None,
		spot_fulfillment_method_fee: None,
		taker: Some(*user_key),
		taker_order_id: Some(user_order_id),
		taker_order_direction: Some(user_position_direction_to_close),
		taker_order_base_asset_amount: Some(base_asset_amount),
		taker_order_cumulative_base_asset_amount_filled: Some(base_asset_amount),
		taker_order_cumulative_quote_asset_amount_filled: Some(base_asset_value),
		maker: Some(*liquidator_key),
		maker_order_id: Some(liquidator_order_id),
		maker_order_direction: Some(user_existing_position_direction),
		maker_order_base_asset_amount: Some(base_asset_amount),
		maker_order_cumulative_base_asset_amount_filled: Some(base_asset_amount),
		maker_order_cumulative_quote_asset_amount_filled: Some(base_asset_value),
		oracle_price,
	};
	emit!(fill_record);

	emit!(LiquidationRecord {
		ts: now,
		liquidation_id,
		liquidation_type: LiquidationType::LiquidatePerp,
		user: *user_key,
		liquidator: *liquidator_key,
		margin_requirement: margin_calculation.margin_requirement,
		total_collateral: margin_calculation.total_collateral,
		bankrupt: user.is_bankrupt(),
		canceled_order_ids,
		margin_freed,
		liquidate_perp: LiquidatePerpRecord {
			market_index,
			oracle_price,
			base_asset_amount: user_position_delta.base_asset_amount,
			quote_asset_amount: user_position_delta.quote_asset_amount,
			lp_shares,
			user_order_id,
			liquidator_order_id,
			fill_record_id,
			liquidator_fee: liquidator_fee.abs().cast()?,
			if_fee: if_fee.abs().cast()?,
		},
		..LiquidationRecord::default()
	});

	Ok(())
}

pub fn resolve_perp_bankruptcy(
	market_index: u16,
	user: &mut User,
	user_key: &Pubkey,
	liquidator: &mut User,
	liquidator_key: &Pubkey,
	perp_market_map: &PerpMarketMap,
	spot_market_map: &SpotMarketMap,
	oracle_map: &mut OracleMap,
	now: i64,
	insurance_fund_vault_balance: u64
) -> DriftResult<u64> {
	if !user.is_bankrupt() && is_user_bankrupt(user) {
		user.enter_bankruptcy();
	}

	validate!(
		user.is_bankrupt(),
		ErrorCode::UserNotBankrupt,
		"user not bankrupt"
	)?;

	validate!(
		!liquidator.is_being_liquidated(),
		ErrorCode::UserIsBeingLiquidated,
		"liquidator being liquidated"
	)?;

	validate!(
		!liquidator.is_bankrupt(),
		ErrorCode::UserBankrupt,
		"liquidator bankrupt"
	)?;

	let market = perp_market_map.get_ref(&market_index)?;

	validate!(
		!market.is_operation_paused(PerpOperation::Liquidation),
		ErrorCode::InvalidLiquidation,
		"Liquidation operation is paused for market {}",
		market_index
	)?;

	drop(market);

	user.get_perp_position(market_index).map_err(|e| {
		msg!("User does not have a position for perp market {}", market_index);
		e
	})?;

	let loss = user
		.get_perp_position(market_index)?
		.quote_asset_amount.cast::<i128>()?;

	validate!(
		loss < 0,
		ErrorCode::InvalidPerpPositionToLiquidate,
		"user must have negative pnl"
	)?;

	let MarginCalculation { margin_requirement, total_collateral, .. } =
		calculate_margin_requirement_and_total_collateral_and_liability_info(
			user,
			perp_market_map,
			spot_market_map,
			oracle_map,
			MarginContext::standard(MarginRequirementType::Maintenance)
		)?;

	// spot market's insurance fund draw attempt here (before social loss)
	// subtract 1 from available insurance_fund_vault_balance so deposits in insurance vault always remains >= 1

	let if_payment = {
		let mut perp_market = perp_market_map.get_ref_mut(&market_index)?;
		let max_insurance_withdraw = perp_market.insurance_claim.quote_max_insurance
			.safe_sub(perp_market.insurance_claim.quote_settled_insurance)?
			.cast::<u128>()?;

		let if_payment = loss
			.unsigned_abs()
			.min(insurance_fund_vault_balance.saturating_sub(1).cast()?)
			.min(max_insurance_withdraw);

		perp_market.insurance_claim.quote_settled_insurance =
			perp_market.insurance_claim.quote_settled_insurance.safe_add(
				if_payment.cast()?
			)?;

		// move if payment to pnl pool
		let spot_market = &mut spot_market_map.get_ref_mut(
			&QUOTE_SPOT_MARKET_INDEX
		)?;
		let oracle_price_data = oracle_map.get_price_data(&spot_market.oracle)?;
		update_spot_market_cumulative_interest(
			spot_market,
			Some(oracle_price_data),
			now
		)?;

		update_spot_balances(
			if_payment,
			&SpotBalanceType::Deposit,
			spot_market,
			&mut perp_market.pnl_pool,
			false
		)?;

		if_payment
	};

	let losses_remaining: i128 = loss.safe_add(if_payment.cast::<i128>()?)?;
	validate!(
		losses_remaining <= 0,
		ErrorCode::InvalidPerpPositionToLiquidate,
		"losses_remaining must be non-positive"
	)?;

	let fee_pool_payment: i128 = if losses_remaining < 0 {
		let perp_market = &mut perp_market_map.get_ref_mut(&market_index)?;
		let spot_market = &mut spot_market_map.get_ref_mut(
			&QUOTE_SPOT_MARKET_INDEX
		)?;
		let fee_pool_tokens = get_fee_pool_tokens(perp_market, spot_market)?;
		msg!("fee_pool_tokens={:?}", fee_pool_tokens);

		losses_remaining.abs().min(fee_pool_tokens.cast()?)
	} else {
		0
	};
	validate!(
		fee_pool_payment >= 0,
		ErrorCode::InvalidPerpPositionToLiquidate,
		"fee_pool_payment must be non-negative"
	)?;

	if fee_pool_payment > 0 {
		let perp_market = &mut perp_market_map.get_ref_mut(&market_index)?;
		let spot_market = &mut spot_market_map.get_ref_mut(
			&QUOTE_SPOT_MARKET_INDEX
		)?;
		msg!("fee_pool_payment={:?}", fee_pool_payment);
		update_spot_balances(
			fee_pool_payment.unsigned_abs(),
			&SpotBalanceType::Borrow,
			spot_market,
			&mut perp_market.amm.fee_pool,
			false
		)?;
	}

	let loss_to_socialize = losses_remaining.safe_add(
		fee_pool_payment.cast::<i128>()?
	)?;
	validate!(
		loss_to_socialize <= 0,
		ErrorCode::InvalidPerpPositionToLiquidate,
		"loss_to_socialize must be non-positive"
	)?;

	let cumulative_funding_rate_delta =
		calculate_funding_rate_deltas_to_resolve_bankruptcy(
			loss_to_socialize,
			perp_market_map.get_ref(&market_index)?.deref()
		)?;

	// socialize loss
	if loss_to_socialize < 0 {
		let mut market = perp_market_map.get_ref_mut(&market_index)?;

		market.amm.total_social_loss = market.amm.total_social_loss.safe_add(
			loss_to_socialize.unsigned_abs()
		)?;

		market.amm.cumulative_funding_rate_long =
			market.amm.cumulative_funding_rate_long.safe_add(
				cumulative_funding_rate_delta
			)?;

		market.amm.cumulative_funding_rate_short =
			market.amm.cumulative_funding_rate_short.safe_sub(
				cumulative_funding_rate_delta
			)?;
	}

	// clear bad debt
	{
		let mut market = perp_market_map.get_ref_mut(&market_index)?;
		let position_index = get_position_index(
			&user.perp_positions,
			market_index
		)?;
		let quote_asset_amount = user.perp_positions
			[position_index].quote_asset_amount;
		update_quote_asset_amount(
			&mut user.perp_positions[position_index],
			&mut market,
			-quote_asset_amount
		)?;

		user.increment_total_socialized_loss(quote_asset_amount.unsigned_abs())?;
	}

	// exit bankruptcy
	if !is_user_bankrupt(user) {
		user.exit_bankruptcy();
	}

	let liquidation_id = user.next_liquidation_id.safe_sub(1)?;

	emit!(LiquidationRecord {
		ts: now,
		liquidation_id,
		liquidation_type: LiquidationType::PerpBankruptcy,
		user: *user_key,
		liquidator: *liquidator_key,
		margin_requirement,
		total_collateral,
		bankrupt: true,
		perp_bankruptcy: PerpBankruptcyRecord {
			market_index,
			if_payment,
			pnl: loss,
			clawback_user: None,
			clawback_user_payment: None,
			cumulative_funding_rate_delta,
		},
		..LiquidationRecord::default()
	});

	if_payment.cast()
}

pub fn set_vault_status_to_being_liquidated(
	vault: &mut Vault,
	perp_market_map: &PerpMarketMap,
	oracle_map: &mut OracleMap,
	slot: u64,
	state: &State
) -> DriftResult {
	validate!(!vault.is_bankrupt(), ErrorCode::UserBankrupt, "vault bankrupt")?;

	validate!(
		!vault.is_being_liquidated(),
		ErrorCode::UserIsBeingLiquidated,
		"vault is already being liquidated"
	)?;

	let liquidation_margin_buffer_ratio = state.liquidation_margin_buffer_ratio;
	let margin_calculation =
		calculate_margin_requirement_and_total_collateral_and_liability_info(
			vault,
			perp_market_map,
			spot_market_map,
			oracle_map,
			MarginContext::liquidation(liquidation_margin_buffer_ratio)
		)?;

	if
		!vault.is_being_liquidated() &&
		margin_calculation.meets_margin_requirement()
	{
		msg!("margin calculation: {:?}", margin_calculation);
		return Err(ErrorCode::SufficientCollateral);
	} else {
		vault.enter_liquidation(slot)?;
	}
	Ok(())
}