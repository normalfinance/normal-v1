use crate::controller::position::OrderSide;
use crate::error::{ NormalResult, ErrorCode };
use crate::math::casting::Cast;
use crate::constants::constants::AUCTION_DERIVE_PRICE_FRACTION;
use crate::math::orders::standardize_price;
use crate::math::safe_math::SafeMath;
use crate::state::oracle::OraclePriceData;
use crate::state::user::{ Order, OrderType };
use solana_program::msg;

use crate::state::fill_mode::FillMode;
use crate::state::market::Market;
use crate::{ OrderParams };
use std::cmp::min;

// #[cfg(test)]
// mod tests;

pub fn calculate_auction_prices(
    oracle_price_data: &OraclePriceData,
    side: OrderSide,
    limit_price: u64
) -> NormalResult<(i64, i64)> {
    let oracle_price = oracle_price_data.price;
    let limit_price = limit_price.cast::<i64>()?;
    if limit_price > 0 {
        let (auction_start_price, auction_end_price) = match side {
            // Long and limit price is better than oracle price
            OrderSide::Buy if limit_price < oracle_price => {
                let limit_derive_start_price = limit_price.safe_sub(
                    limit_price / AUCTION_DERIVE_PRICE_FRACTION
                )?;
                let oracle_derive_start_price = oracle_price.safe_sub(
                    oracle_price / AUCTION_DERIVE_PRICE_FRACTION
                )?;

                (limit_derive_start_price.min(oracle_derive_start_price), limit_price)
            }
            // Long and limit price is worse than oracle price
            OrderSide::Buy if limit_price >= oracle_price => {
                let oracle_derive_end_price = oracle_price.safe_add(
                    oracle_price / AUCTION_DERIVE_PRICE_FRACTION
                )?;

                (oracle_price, limit_price.min(oracle_derive_end_price))
            }
            // Short and limit price is better than oracle price
            OrderSide::Sell if limit_price > oracle_price => {
                let limit_derive_start_price = limit_price.safe_add(
                    limit_price / AUCTION_DERIVE_PRICE_FRACTION
                )?;
                let oracle_derive_start_price = oracle_price.safe_add(
                    oracle_price / AUCTION_DERIVE_PRICE_FRACTION
                )?;

                (limit_derive_start_price.max(oracle_derive_start_price), limit_price)
            }
            // Short and limit price is worse than oracle price
            OrderSide::Sell if limit_price <= oracle_price => {
                let oracle_derive_end_price = oracle_price.safe_sub(
                    oracle_price / AUCTION_DERIVE_PRICE_FRACTION
                )?;

                (oracle_price, limit_price.max(oracle_derive_end_price))
            }
            _ => unreachable!(),
        };

        return Ok((auction_start_price, auction_end_price));
    }

    let auction_end_price = match side {
        OrderSide::Buy => {
            oracle_price.safe_add(oracle_price / AUCTION_DERIVE_PRICE_FRACTION)?
        }
        OrderSide::Sell => {
            oracle_price.safe_sub(oracle_price / AUCTION_DERIVE_PRICE_FRACTION)?
        }
    };

    Ok((oracle_price, auction_end_price))
}

pub fn calculate_auction_price(
    order: &Order,
    slot: u64,
    tick_size: u64,
    valid_oracle_price: Option<i64>
) -> NormalResult<u64> {
    match order.order_type {
        | OrderType::Market
        | OrderType::TriggerMarket
        | OrderType::Limit
        | OrderType::TriggerLimit => {
            calculate_auction_price_for_fixed_auction(order, slot, tick_size)
        }
    }
}

fn calculate_auction_price_for_fixed_auction(
    order: &Order,
    slot: u64,
    tick_size: u64
) -> NormalResult<u64> {
    let slots_elapsed = slot.safe_sub(order.slot)?;

    let delta_numerator = min(slots_elapsed, order.auction_duration.cast()?);
    let delta_denominator = order.auction_duration;

    let auction_start_price = order.auction_start_price.cast::<u64>()?;
    let auction_end_price = order.auction_end_price.cast::<u64>()?;

    if delta_denominator == 0 {
        return standardize_price(auction_end_price, tick_size, order.side);
    }

    let price_delta = match order.side {
        OrderSide::Buy =>
            auction_end_price
                .safe_sub(auction_start_price)?
                .safe_mul(delta_numerator.cast()?)?
                .safe_div(delta_denominator.cast()?)?,
        OrderSide::Sell =>
            auction_start_price
                .safe_sub(auction_end_price)?
                .safe_mul(delta_numerator.cast()?)?
                .safe_div(delta_denominator.cast()?)?,
    };

    let price = match order.side {
        OrderSide::Buy => auction_start_price.safe_add(price_delta)?,
        OrderSide::Sell => auction_start_price.safe_sub(price_delta)?,
    };

    standardize_price(price, tick_size, order.side)
}

pub fn is_auction_complete(order_slot: u64, auction_duration: u8, slot: u64) -> NormalResult<bool> {
    if auction_duration == 0 {
        return Ok(true);
    }

    let slots_elapsed = slot.safe_sub(order_slot)?;

    Ok(slots_elapsed > auction_duration.cast()?)
}

pub fn is_amm_available_liquidity_source(
    order: &Order,
    min_auction_duration: u8,
    slot: u64,
    fill_mode: FillMode
) -> NormalResult<bool> {
    Ok(is_auction_complete(order.slot, min_auction_duration, slot)?)
}

pub fn calculate_auction_params_for_trigger_order(
    order: &Order,
    oracle_price_data: &OraclePriceData,
    min_auction_duration: u8,
    market: Option<&Market>
) -> NormalResult<(u8, i64, i64)> {
    let auction_duration = min_auction_duration;

    if let Some(market) = market {
        let (auction_start_price, auction_end_price, derived_auction_duration) =
            OrderParams::derive_market_order_auction_params(
                market,
                order.side,
                oracle_price_data.price,
                order.price,
                0
            )?;

        let auction_duration = auction_duration.max(derived_auction_duration);

        Ok((auction_duration, auction_start_price, auction_end_price))
    } else {
        let (auction_start_price, auction_end_price) = calculate_auction_prices(
            oracle_price_data,
            order.side,
            order.price
        )?;

        Ok((auction_duration, auction_start_price, auction_end_price))
    }
}
