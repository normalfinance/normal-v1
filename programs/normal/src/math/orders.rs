use std::cmp::min;
use std::ops::Sub;

use solana_program::msg;

use crate::controller::position::PositionDelta;
use crate::controller::position::OrderSide;
use crate::error::{ NormalResult, ErrorCode };
use crate::math::amm::calculate_amm_available_liquidity;
use crate::math::auction::is_amm_available_liquidity_source;
use crate::math::casting::Cast;
use crate::state::fill_mode::FillMode;
use crate::{
    load,
    math,
    FeeTier,
    State,
    BASE_PRECISION_I128,
    FEE_ADJUSTMENT_MAX,
    PERCENTAGE_PRECISION,
    PERCENTAGE_PRECISION_U64,
    PRICE_PRECISION_I128,
    PRICE_PRECISION_U64,
    QUOTE_PRECISION_I128,
};

use crate::math::safe_math::SafeMath;
use crate::math_error;
use crate::print_error;
use crate::state::oracle::{ OraclePriceData, StrictOraclePrice };
use crate::state::oracle_map::OracleMap;
use crate::state::order_params::PostOnlyParam;
use crate::state::market::{ Market, AMM };
use crate::state::market_map::MarketMap;
use crate::state::user::{
    MarketType,
    Order,
    OrderFillSimulation,
    OrderStatus,
    OrderTriggerCondition,
    Position,
    User,
};
use crate::state::user_map::UserMap;
use crate::validate;

// #[cfg(test)]
// mod tests;

pub fn calculate_base_asset_amount_for_amm_to_fulfill(
    order: &Order,
    market: &Market,
    limit_price: Option<u64>,
    override_fill_price: Option<u64>,
    existing_base_asset_amount: u64,
    fee_tier: &FeeTier
) -> NormalResult<(u64, Option<u64>)> {
    let limit_price = if let Some(override_fill_price) = override_fill_price {
        if let Some(limit_price) = limit_price {
            validate!(
                (limit_price >= override_fill_price &&
                    order.side == OrderSide::Buy) ||
                    (limit_price <= override_fill_price &&
                        order.side == OrderSide::Sell),
                ErrorCode::InvalidAmmLimitPriceOverride,
                "override_limit_price={} not better than order_limit_price={}",
                override_fill_price,
                limit_price
            )?;
        }

        Some(override_fill_price)
    } else {
        limit_price
    };

    if order.must_be_triggered() && !order.triggered() {
        return Ok((0, limit_price));
    }

    let limit_price_with_buffer = calculate_limit_price_with_buffer(
        order,
        limit_price,
        fee_tier,
        market.fee_adjustment
    )?;

    let base_asset_amount = calculate_base_asset_amount_to_fill_up_to_limit_price(
        order,
        market,
        limit_price_with_buffer,
        Some(existing_base_asset_amount)
    )?;
    let max_base_asset_amount = calculate_amm_available_liquidity(&market.amm, &order.side)?;

    Ok((min(base_asset_amount, max_base_asset_amount), limit_price))
}

fn calculate_limit_price_with_buffer(
    order: &Order,
    limit_price: Option<u64>,
    fee_tier: &FeeTier,
    fee_adjustment: i16
) -> NormalResult<Option<u64>> {
    if !order.post_only {
        Ok(limit_price)
    } else if let Some(limit_price) = limit_price {
        let mut buffer = limit_price
            .safe_mul(fee_tier.maker_rebate_numerator.cast()?)?
            .safe_div(fee_tier.maker_rebate_denominator.cast()?)?;

        if fee_adjustment < 0 {
            let buffer_adjustment = buffer
                .safe_mul(fee_adjustment.abs().cast()?)?
                .safe_div(FEE_ADJUSTMENT_MAX)?;
            buffer = buffer.saturating_sub(buffer_adjustment);
        } else if fee_adjustment > 0 {
            let buffer_adjustment = buffer
                .safe_mul(fee_adjustment.cast()?)?
                .safe_div(FEE_ADJUSTMENT_MAX)?;
            buffer = buffer.saturating_add(buffer_adjustment);
        }

        match order.side {
            OrderSide::Buy => limit_price.safe_sub(buffer).map(Some),
            OrderSide::Sell => limit_price.safe_add(buffer).map(Some),
        }
    } else {
        Ok(None)
    }
}

pub fn calculate_base_asset_amount_to_fill_up_to_limit_price(
    order: &Order,
    market: &Market,
    limit_price: Option<u64>,
    existing_base_asset_amount: Option<u64>
) -> NormalResult<u64> {
    let base_asset_amount_unfilled = order.get_base_asset_amount_unfilled(
        existing_base_asset_amount
    )?;

    let (max_trade_base_asset_amount, max_trade_side) = if let Some(limit_price) = limit_price {
        // buy to right below or sell up right above the limit price
        let adjusted_limit_price = match order.side {
            OrderSide::Buy => limit_price.safe_sub(market.amm.order_tick_size)?,
            OrderSide::Sell => limit_price.safe_add(market.amm.order_tick_size)?,
        };

        math::amm_spread::calculate_base_asset_amount_to_trade_to_price(
            &market.amm,
            adjusted_limit_price,
            order.side
        )?
    } else {
        (base_asset_amount_unfilled, order.side)
    };

    if max_trade_side != order.side || max_trade_base_asset_amount == 0 {
        return Ok(0);
    }

    standardize_base_asset_amount(
        min(base_asset_amount_unfilled, max_trade_base_asset_amount),
        market.amm.order_step_size
    )
}

pub fn calculate_quote_asset_amount_for_maker_order(
    base_asset_amount: u64,
    fill_price: u64,
    base_decimals: u32,
    position_side: OrderSide
) -> NormalResult<u64> {
    let precision_decrease = (10_u128).pow(base_decimals);

    match position_side {
        OrderSide::Buy =>
            fill_price
                .cast::<u128>()?
                .safe_mul(base_asset_amount.cast()?)?
                .safe_div(precision_decrease)?
                .cast::<u64>(),
        OrderSide::Sell =>
            fill_price
                .cast::<u128>()?
                .safe_mul(base_asset_amount.cast()?)?
                .safe_div_ceil(precision_decrease)?
                .cast::<u64>(),
    }
}

pub fn standardize_base_asset_amount_with_remainder_i128(
    base_asset_amount: i128,
    step_size: u128
) -> NormalResult<(i128, i128)> {
    let remainder = base_asset_amount
        .unsigned_abs()
        .checked_rem_euclid(step_size)
        .ok_or_else(math_error!())?
        .cast::<i128>()?
        .safe_mul(base_asset_amount.signum())?;

    let standardized_base_asset_amount = base_asset_amount.safe_sub(remainder)?;

    Ok((standardized_base_asset_amount, remainder))
}

pub fn standardize_base_asset_amount(base_asset_amount: u64, step_size: u64) -> NormalResult<u64> {
    let remainder = base_asset_amount.checked_rem_euclid(step_size).ok_or_else(math_error!())?;

    base_asset_amount.safe_sub(remainder)
}

pub fn standardize_base_asset_amount_ceil(
    base_asset_amount: u64,
    step_size: u64
) -> NormalResult<u64> {
    let remainder = base_asset_amount.checked_rem_euclid(step_size).ok_or_else(math_error!())?;

    if remainder == 0 {
        Ok(base_asset_amount)
    } else {
        base_asset_amount.safe_add(step_size)?.safe_sub(remainder)
    }
}

pub fn is_multiple_of_step_size(base_asset_amount: u64, step_size: u64) -> NormalResult<bool> {
    let remainder = base_asset_amount.checked_rem_euclid(step_size).ok_or_else(math_error!())?;

    Ok(remainder == 0)
}

pub fn standardize_price(
    price: u64,
    tick_size: u64,
    side: OrderSide
) -> NormalResult<u64> {
    if price == 0 {
        return Ok(0);
    }

    let remainder = price.checked_rem_euclid(tick_size).ok_or_else(math_error!())?;

    if remainder == 0 {
        return Ok(price);
    }

    match side {
        OrderSide::Buy => price.safe_sub(remainder),
        OrderSide::Sell => price.safe_add(tick_size)?.safe_sub(remainder),
    }
}

pub fn standardize_price_i64(
    price: i64,
    tick_size: i64,
    side: OrderSide
) -> NormalResult<i64> {
    if price == 0 {
        return Ok(0);
    }

    let remainder = price.checked_rem_euclid(tick_size).ok_or_else(math_error!())?;

    if remainder == 0 {
        return Ok(price);
    }

    match side {
        OrderSide::Buy => price.safe_sub(remainder),
        OrderSide::Sell => price.safe_add(tick_size)?.safe_sub(remainder),
    }
}

pub fn get_price_for_order(
    price: u64,
    side: OrderSide,
    post_only: PostOnlyParam,
    amm: &AMM
) -> NormalResult<u64> {
    let mut limit_price = standardize_price(price, amm.order_tick_size, side)?;

    if post_only == PostOnlyParam::Slide {
        let reserve_price = amm.reserve_price()?;
        match side {
            OrderSide::Buy => {
                let amm_ask = amm.ask_price(reserve_price)?;
                if limit_price >= amm_ask {
                    limit_price = amm_ask.safe_sub(amm.order_tick_size)?;
                }
            }
            OrderSide::Sell => {
                let amm_bid = amm.bid_price(reserve_price)?;
                if limit_price <= amm_bid {
                    limit_price = amm_bid.safe_add(amm.order_tick_size)?;
                }
            }
        }
    }

    Ok(limit_price)
}

pub fn get_position_delta_for_fill(
    base_asset_amount: u64,
    quote_asset_amount: u64,
    side: OrderSide
) -> NormalResult<PositionDelta> {
    Ok(PositionDelta {
        quote_asset_amount: match side {
            OrderSide::Buy => -quote_asset_amount.cast()?,
            OrderSide::Sell => quote_asset_amount.cast()?,
        },
        base_asset_amount: match side {
            OrderSide::Buy => base_asset_amount.cast()?,
            OrderSide::Sell => -base_asset_amount.cast()?,
        },
        remainder_base_asset_amount: None,
    })
}

#[inline(always)]
pub fn validate_fill_possible(
    state: &State,
    user: &User,
    order_index: usize,
    slot: u64,
    num_makers: usize,
    fill_mode: FillMode
) -> NormalResult {
    let amm_available = is_amm_available_liquidity_source(
        &user.orders[order_index],
        state.min_auction_duration,
        slot,
        fill_mode
    )?;

    if !amm_available && num_makers == 0 && user.orders[order_index].is_limit_order() {
        msg!("invalid fill. order is limit order, amm is not available and no makers present");
        return Err(ErrorCode::ImpossibleFill);
    }

    Ok(())
}

#[inline(always)]
pub fn should_expire_order_before_fill(
    user: &User,
    order_index: usize,
    now: i64
) -> NormalResult<bool> {
    let should_order_be_expired = should_expire_order(user, order_index, now)?;
    if should_order_be_expired && user.orders[order_index].is_limit_order() {
        let now_sub_buffer = now.safe_sub(15)?;
        if !should_expire_order(user, order_index, now_sub_buffer)? {
            msg!(
                "invalid fill. cant force expire limit order until 15s after max_ts. max ts {}, now {}, now plus buffer {}",
                user.orders[order_index].max_ts,
                now,
                now_sub_buffer
            );
            return Err(ErrorCode::ImpossibleFill);
        }
    }

    Ok(should_order_be_expired)
}

#[inline(always)]
pub fn should_expire_order(user: &User, user_order_index: usize, now: i64) -> NormalResult<bool> {
    let order = &user.orders[user_order_index];
    if order.status != OrderStatus::Open || order.max_ts == 0 || order.must_be_triggered() {
        return Ok(false);
    }

    Ok(now > order.max_ts)
}

pub fn should_cancel_reduce_only_order(
    order: &Order,
    existing_base_asset_amount: i64,
    step_size: u64
) -> NormalResult<bool> {
    let should_cancel =
        order.status == OrderStatus::Open &&
        order.reduce_only &&
        order.get_base_asset_amount_unfilled(Some(existing_base_asset_amount))? < step_size;

    Ok(should_cancel)
}

pub fn validate_fill_price_within_price_bands(
    fill_price: u64,
    side: OrderSide,
    oracle_price: i64,
    oracle_twap_5min: i64,
    oracle_twap_5min_percent_divergence: u64
) -> NormalResult {
    let oracle_price = oracle_price.unsigned_abs();
    let oracle_twap_5min = oracle_twap_5min.unsigned_abs();

    // let max_oracle_diff = margin_ratio_initial.cast::<u128>()?;
    let max_oracle_twap_diff = oracle_twap_5min_percent_divergence.cast::<u128>()?; // 50%

    if side == OrderSide::Buy {
        if fill_price < oracle_price && fill_price < oracle_twap_5min {
            return Ok(());
        }

        let percent_diff: u128 = fill_price
            .saturating_sub(oracle_price)
            .cast::<u128>()?
            .safe_mul(MARGIN_PRECISION_U128)?
            .safe_div(oracle_price.cast()?)?;

        // validate!(
        //     percent_diff < max_oracle_diff,
        //     ErrorCode::PriceBandsBreached,
        //     "Fill Price Breaches Oracle Price Bands: {} % <= {} % (fill: {} >= oracle: {})",
        //     max_oracle_diff,
        //     percent_diff,
        //     fill_price,
        //     oracle_price
        // )?;

        let percent_diff = fill_price
            .saturating_sub(oracle_twap_5min)
            .cast::<u128>()?
            .safe_mul(PERCENTAGE_PRECISION)?
            .safe_div(oracle_twap_5min.cast()?)?;

        validate!(
            percent_diff < max_oracle_twap_diff,
            ErrorCode::PriceBandsBreached,
            "Fill Price Breaches Oracle TWAP Price Bands:  {} % <= {} % (fill: {} >= twap: {})",
            max_oracle_twap_diff,
            percent_diff,
            fill_price,
            oracle_twap_5min
        )?;
    } else {
        if fill_price > oracle_price && fill_price > oracle_twap_5min {
            return Ok(());
        }

        let percent_diff: u128 = oracle_price
            .saturating_sub(fill_price)
            .cast::<u128>()?
            .safe_mul(MARGIN_PRECISION_U128)?
            .safe_div(oracle_price.cast()?)?;

        // validate!(
        //     percent_diff < max_oracle_diff,
        //     ErrorCode::PriceBandsBreached,
        //     "Fill Price Breaches Oracle Price Bands: {} % <= {} % (fill: {} <= oracle: {})",
        //     max_oracle_diff,
        //     percent_diff,
        //     fill_price,
        //     oracle_price
        // )?;

        let percent_diff = oracle_twap_5min
            .saturating_sub(fill_price)
            .cast::<u128>()?
            .safe_mul(PERCENTAGE_PRECISION)?
            .safe_div(oracle_twap_5min.cast()?)?;

        validate!(
            percent_diff < max_oracle_twap_diff,
            ErrorCode::PriceBandsBreached,
            "Fill Price Breaches Oracle TWAP Price Bands:  {} % <= {} % (fill: {} <= twap: {})",
            max_oracle_twap_diff,
            percent_diff,
            fill_price,
            oracle_twap_5min
        )?;
    }

    Ok(())
}

pub fn is_oracle_too_divergent_with_twap_5min(
    oracle_price: i64,
    oracle_twap_5min: i64,
    max_divergence: i64
) -> NormalResult<bool> {
    let percent_diff = oracle_price
        .safe_sub(oracle_twap_5min)?
        .abs()
        .safe_mul(PERCENTAGE_PRECISION_U64.cast::<i64>()?)?
        .safe_div(oracle_twap_5min.abs())?;

    let too_divergent = percent_diff >= max_divergence;
    if too_divergent {
        msg!("max divergence {}", max_divergence);
        msg!(
            "Oracle Price Too Divergent from TWAP 5min. oracle: {} twap: {}",
            oracle_price,
            oracle_twap_5min
        );
    }

    Ok(too_divergent)
}

pub fn order_satisfies_trigger_condition(order: &Order, oracle_price: u64) -> NormalResult<bool> {
    match order.trigger_condition {
        OrderTriggerCondition::Above => Ok(oracle_price > order.trigger_price),
        OrderTriggerCondition::Below => Ok(oracle_price < order.trigger_price),
        _ => Err(print_error!(ErrorCode::InvalidTriggerOrderCondition)()),
    }
}

pub fn is_new_order_risk_increasing(
    order: &Order,
    position_base_asset_amount: i64,
    position_bids: i64,
    position_asks: i64
) -> NormalResult<bool> {
    if order.reduce_only {
        return Ok(false);
    }

    match order.side {
        OrderSide::Buy => {
            if position_base_asset_amount >= 0 {
                return Ok(true);
            }

            Ok(
                position_bids.safe_add(order.base_asset_amount.cast()?)? >
                    position_base_asset_amount.abs()
            )
        }
        OrderSide::Sell => {
            if position_base_asset_amount <= 0 {
                return Ok(true);
            }

            Ok(
                position_asks.safe_sub(order.base_asset_amount.cast()?)?.abs() >
                    position_base_asset_amount
            )
        }
    }
}

pub fn is_order_position_reducing(
    order_side: &OrderSide,
    order_base_asset_amount: u64,
    position_base_asset_amount: i64
) -> NormalResult<bool> {
    Ok(match order_side {
        // User is short and order is long
        OrderSide::Buy if position_base_asset_amount < 0 => {
            order_base_asset_amount <= position_base_asset_amount.unsigned_abs()
        }
        // User is long and order is short
        OrderSide::Sell if position_base_asset_amount > 0 => {
            order_base_asset_amount <= position_base_asset_amount.unsigned_abs()
        }
        _ => false,
    })
}

pub fn validate_fill_price(
    quote_asset_amount: u64,
    base_asset_amount: u64,
    base_precision: u64,
    order_side: OrderSide,
    order_limit_price: u64,
    is_taker: bool
) -> NormalResult {
    let rounded_quote_asset_amount = if is_taker {
        match order_side {
            OrderSide::Buy => quote_asset_amount.saturating_sub(1),
            OrderSide::Sell => quote_asset_amount.saturating_add(1),
        }
    } else {
        quote_asset_amount
    };

    let fill_price = calculate_fill_price(
        rounded_quote_asset_amount,
        base_asset_amount,
        base_precision
    )?;

    if order_side == OrderSide::Buy && fill_price > order_limit_price {
        msg!(
            "long order fill price ({} = {}/{} * 1000) > limit price ({}) is_taker={}",
            fill_price,
            quote_asset_amount,
            base_asset_amount,
            order_limit_price,
            is_taker
        );
        return Err(ErrorCode::InvalidOrderFillPrice);
    }

    if order_side == OrderSide::Sell && fill_price < order_limit_price {
        msg!(
            "short order fill price ({} = {}/{} * 1000) < limit price ({}) is_taker={}",
            fill_price,
            quote_asset_amount,
            base_asset_amount,
            order_limit_price,
            is_taker
        );
        return Err(ErrorCode::InvalidOrderFillPrice);
    }

    Ok(())
}

pub fn calculate_fill_price(
    quote_asset_amount: u64,
    base_asset_amount: u64,
    base_precision: u64
) -> NormalResult<u64> {
    quote_asset_amount
        .cast::<u128>()?
        .safe_mul(base_precision as u128)?
        .safe_div(base_asset_amount.cast()?)?
        .cast::<u64>()
}

pub fn find_maker_orders(
    user: &User,
    side: &OrderSide,
    market_type: &MarketType,
    market_index: u16,
    valid_oracle_price: Option<i64>,
    slot: u64,
    tick_size: u64
) -> NormalResult<Vec<(usize, u64)>> {
    let mut orders: Vec<(usize, u64)> = Vec::with_capacity(32);

    for (order_index, order) in user.orders.iter().enumerate() {
        if order.status != OrderStatus::Open {
            continue;
        }

        // if order side is not same or market type is not same or market index is the same, skip
        if
            order.side != *side ||
            order.market_type != *market_type ||
            order.market_index != market_index
        {
            continue;
        }

        // if order is not limit order or must be triggered and not triggered, skip
        if !order.is_limit_order() || (order.must_be_triggered() && !order.triggered()) {
            continue;
        }

        let limit_price = order.force_get_limit_price(valid_oracle_price, None, slot, tick_size)?;

        orders.push((order_index, limit_price));
    }

    Ok(orders)
}

pub fn calculate_max_order_size(
    user: &User,
    position_index: usize,
    market_index: u16,
    side: OrderSide,
    market_map: &MarketMap,
    oracle_map: &mut OracleMap
) -> NormalResult<u64> {
    let market = market_map.get_ref(&market_index)?;

    let oracle_price_data_price = oracle_map.get_price_data(&market.amm.oracle)?.price;

    let quote_oracle_price = oracle_map
        .get_price_data(&market.amm.oracle)?
        .price.max(market.historical_oracle_data.last_oracle_price_twap_5min);

    let position: &Position = &user.positions[position_index];
    let (worst_case_base_asset_amount, worst_case_liability_value) =
        position.worst_case_liability_value(oracle_price_data_price)?;

    let mut order_size_to_reduce_position = 0_u64;
    // account for order flipping worst case base asset amount
    if worst_case_base_asset_amount < 0 && side == OrderSide::Buy {
        order_size_to_reduce_position = worst_case_base_asset_amount
            .abs()
            .cast::<i64>()?
            .safe_sub(position.open_bids)?
            .max(0)
            .unsigned_abs();
    } else if worst_case_base_asset_amount > 0 && side == OrderSide::Sell {
        order_size_to_reduce_position = worst_case_base_asset_amount
            .cast::<i64>()?
            .safe_add(position.open_asks)?
            .max(0)
            .unsigned_abs();
    }

    standardize_base_asset_amount(
        order_size.safe_add(order_size_to_reduce_position)?,
        market.amm.order_step_size
    )
}

#[derive(Eq, PartialEq, Debug)]
pub struct Level {
    pub price: u64,
    pub base_asset_amount: u64,
}

pub fn find_bids_and_asks_from_users(
    market: &Market,
    oracle_price_date: &OraclePriceData,
    users: &UserMap,
    slot: u64,
    now: i64
) -> NormalResult<(Vec<Level>, Vec<Level>)> {
    let mut bids: Vec<Level> = Vec::with_capacity(32);
    let mut asks: Vec<Level> = Vec::with_capacity(32);

    let market_index = market.market_index;
    let tick_size = market.amm.order_tick_size;
    let oracle_price = Some(oracle_price_date.price);

    let mut insert_order = |base_asset_amount: u64, price: u64, side: OrderSide| {
        let orders = match side {
            OrderSide::Buy => &mut bids,
            OrderSide::Sell => &mut asks,
        };
        let index = match
            orders.binary_search_by(|level| {
                match side {
                    OrderSide::Buy => price.cmp(&level.price),
                    OrderSide::Sell => level.price.cmp(&price),
                }
            })
        {
            Ok(index) => index,
            Err(index) => index,
        };

        if index < orders.capacity() {
            if orders.len() == orders.capacity() {
                orders.pop();
            }

            orders.insert(index, Level {
                price,
                base_asset_amount,
            });
        }
    };

    for account_loader in users.0.values() {
        let user = load!(account_loader)?;

        for (_, order) in user.orders.iter().enumerate() {
            if order.status != OrderStatus::Open {
                continue;
            }

            if order.market_type != MarketType::Synthetic || order.market_index != market_index {
                continue;
            }

            // if order is not limit order or must be triggered and not triggered, skip
            if !order.is_limit_order() || (order.must_be_triggered() && !order.triggered()) {
                continue;
            }

            if !order.is_resting_limit_order(slot)? {
                continue;
            }

            if now > order.max_ts && order.max_ts != 0 {
                continue;
            }

            let existing_position = user.get_position(market_index)?.base_asset_amount;
            let base_amount = order.get_base_asset_amount_unfilled(Some(existing_position))?;
            let limit_price = order.force_get_limit_price(oracle_price, None, slot, tick_size)?;

            insert_order(base_amount, limit_price, order.side);
        }
    }

    Ok((bids, asks))
}

pub fn estimate_price_from_side(side: &Vec<Level>, depth: u64) -> NormalResult<Option<u64>> {
    let mut depth_remaining = depth;
    let mut cumulative_base = 0_u64;
    let mut cumulative_quote = 0_u128;

    for level in side {
        let base_delta = level.base_asset_amount.min(depth_remaining);
        let quote_delta = level.price.cast::<u128>()?.safe_mul(base_delta.cast()?)?;

        cumulative_base = cumulative_base.safe_add(base_delta)?;
        depth_remaining = depth_remaining.safe_sub(base_delta)?;
        cumulative_quote = cumulative_quote.safe_add(quote_delta)?;

        if depth_remaining == 0 {
            break;
        }
    }

    let price = if depth_remaining == 0 {
        Some(cumulative_quote.safe_div(cumulative_base.cast()?)?.cast::<u64>()?)
    } else {
        None
    };

    Ok(price)
}
