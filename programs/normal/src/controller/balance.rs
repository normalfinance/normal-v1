use crate::math::oracle::oracle_validity;
use crate::state::state::ValidityGuardRails;
use std::cmp::max; //, OracleValidity};

use anchor_lang::prelude::*;
use solana_program::msg;

use crate::error::{NormalResult, ErrorCode};
use crate::math::amm::sanitize_new_price;
use crate::math::casting::Cast;
use crate::constants::constants::{
    FIVE_MINUTE, IF_FACTOR_PRECISION, ONE_HOUR, QUOTE_SPOT_MARKET_INDEX,
    SPOT_MARKET_TOKEN_TWAP_WINDOW,
};
use crate::math::balance::{
    calculate_accumulated_interest, calculate_utilization, get_interest_token_amount,
    get_spot_balance, get_token_amount, InterestAccumulated,
};
use crate::math::stats::{calculate_new_twap, calculate_weighted_average};

use crate::math::oracle::{is_oracle_valid_for_action, NormalAction};
use crate::math::safe_math::SafeMath;
use crate::state::events::SpotInterestRecord;
use crate::state::oracle::OraclePriceData;
use crate::state::paused_operations::Operation;
use crate::state::market::{Balance, BalanceType, Market};
use crate::state::user::MarketType;
use crate::validate;

// #[cfg(test)]
// mod tests;

pub fn update_spot_market_twap_stats(
    spot_market: &mut SpotMarket,
    oracle_price_data: Option<&OraclePriceData>,
    now: i64,
) -> NormalResult {
    let since_last = max(0_i64, now.safe_sub(spot_market.last_twap_ts.cast()?)?);
    let from_start = max(1_i64, SPOT_MARKET_TOKEN_TWAP_WINDOW.safe_sub(since_last)?);

    let deposit_token_amount = get_token_amount(
        spot_market.deposit_balance,
        spot_market,
        &SpotBalanceType::Deposit,
    )?;

    let borrow_token_amount = get_token_amount(
        spot_market.borrow_balance,
        spot_market,
        &SpotBalanceType::Borrow,
    )?;

    spot_market.deposit_token_twap = calculate_weighted_average(
        deposit_token_amount.cast()?,
        spot_market.deposit_token_twap.cast()?,
        since_last,
        from_start,
    )?
    .cast()?;

    spot_market.borrow_token_twap = calculate_weighted_average(
        borrow_token_amount.cast()?,
        spot_market.borrow_token_twap.cast()?,
        since_last,
        from_start,
    )?
    .cast()?;

    let utilization = calculate_utilization(deposit_token_amount, borrow_token_amount)?;

    spot_market.utilization_twap = calculate_weighted_average(
        utilization.cast()?,
        spot_market.utilization_twap.cast()?,
        since_last,
        from_start,
    )?
    .cast()?;

    if let Some(oracle_price_data) = oracle_price_data {
        let sanitize_clamp_denominator = spot_market.get_sanitize_clamp_denominator()?;

        let capped_oracle_update_price: i64 = sanitize_new_price(
            oracle_price_data.price,
            spot_market.historical_oracle_data.last_oracle_price_twap,
            sanitize_clamp_denominator,
        )?;

        let oracle_price_twap = calculate_new_twap(
            capped_oracle_update_price,
            now,
            spot_market.historical_oracle_data.last_oracle_price_twap,
            spot_market.historical_oracle_data.last_oracle_price_twap_ts,
            ONE_HOUR,
        )?;

        let oracle_price_twap_5min = calculate_new_twap(
            capped_oracle_update_price,
            now,
            spot_market
                .historical_oracle_data
                .last_oracle_price_twap_5min,
            spot_market.historical_oracle_data.last_oracle_price_twap_ts,
            FIVE_MINUTE as i64,
        )?;

        spot_market.historical_oracle_data.last_oracle_price_twap = oracle_price_twap;
        spot_market
            .historical_oracle_data
            .last_oracle_price_twap_5min = oracle_price_twap_5min;

        spot_market.historical_oracle_data.last_oracle_price = oracle_price_data.price;
        spot_market.historical_oracle_data.last_oracle_conf = oracle_price_data.confidence;
        spot_market.historical_oracle_data.last_oracle_delay = oracle_price_data.delay;
        spot_market.historical_oracle_data.last_oracle_price_twap_ts = now;
    }

    spot_market.last_twap_ts = now.cast()?;

    Ok(())
}


pub fn update_fee_pool_balances(
    token_amount: u128,
    update_direction: &BalanceType,
    market: &mut Market,
) -> NormalResult {
    let mut balance = market.fee_pool;
    update_balances(
        token_amount,
        update_direction,
        market,
        &mut balance,
        false,
    )?;
    market.fee_pool = balance;

    Ok(())
}

pub fn update_balances(
    mut token_amount: u128,
    update_direction: &BalanceType,
    market: &mut Market,
    balance: &mut dyn Balance,
    is_leaving_normal: bool,
) -> NormalResult {
    let increase_user_existing_balance = update_direction == balance.balance_type();
    if increase_user_existing_balance {
        let round_up = balance.balance_type() == &BalanceType::Borrow;
        let balance_delta =
            get_balance(token_amount, market, update_direction, round_up)?;
        balance.increase_balance(balance_delta)?;
        increase_balance(balance_delta, market, update_direction)?;
    } else {
        let current_token_amount = get_token_amount(
            balance.balance(),
            market,
            balance.balance_type(),
        )?;

        let reduce_user_existing_balance = current_token_amount != 0;
        if reduce_user_existing_balance {
            // determine how much to reduce balance based on size of current token amount
            let (token_delta, balance_delta) = if current_token_amount > token_amount {
                let round_up =
                    is_leaving_normal || balance.balance_type() == &BalanceType::Borrow;
                let balance_delta = get_balance(
                    token_amount,
                    market,
                    balance.balance_type(),
                    round_up,
                )?;
                (token_amount, balance_delta)
            } else {
                (current_token_amount, balance.balance())
            };

            decrease_balance(balance_delta, market, balance.balance_type())?;
            balance.decrease_balance(balance_delta)?;
            token_amount = token_amount.safe_sub(token_delta)?;
        }

        if token_amount > 0 {
            balance.update_balance_type(*update_direction)?;
            let round_up = update_direction == &BalanceType::Borrow;
            let balance_delta =
                get_balance(token_amount, market, update_direction, round_up)?;
            balance.increase_balance(balance_delta)?;
            increase_balance(balance_delta, market, update_direction)?;
        }
    }

    Ok(())
}

pub fn transfer_spot_balances(
    token_amount: i128,
    spot_market: &mut SpotMarket,
    from_spot_balance: &mut dyn SpotBalance,
    to_spot_balance: &mut dyn SpotBalance,
) -> NormalResult {
    validate!(
        from_spot_balance.market_index() == to_spot_balance.market_index(),
        ErrorCode::UnequalMarketIndexForSpotTransfer,
        "transfer market indexes arent equal",
    )?;

    if token_amount == 0 {
        return Ok(());
    }

    if from_spot_balance.balance_type() == &SpotBalanceType::Deposit {
        validate!(
            spot_market.deposit_balance >= from_spot_balance.balance(),
            ErrorCode::InvalidSpotMarketState,
            "spot_market.deposit_balance={} lower than individual spot balance={}",
            spot_market.deposit_balance,
            from_spot_balance.balance()
        )?;
    }

    update_spot_balances(
        token_amount.unsigned_abs(),
        if token_amount < 0 {
            &SpotBalanceType::Deposit
        } else {
            &SpotBalanceType::Borrow
        },
        spot_market,
        from_spot_balance,
        false,
    )?;

    update_spot_balances(
        token_amount.unsigned_abs(),
        if token_amount < 0 {
            &SpotBalanceType::Borrow
        } else {
            &SpotBalanceType::Deposit
        },
        spot_market,
        to_spot_balance,
        false,
    )?;

    Ok(())
}


pub fn update_spot_market_and_check_validity(
    spot_market: &mut SpotMarket,
    oracle_price_data: &OraclePriceData,
    validity_guard_rails: &ValidityGuardRails,
    now: i64,
    action: Option<NormalAction>,
) -> NormalResult {
    // update spot market EMAs with new/current data
    update_spot_market_cumulative_interest(spot_market, Some(oracle_price_data), now)?;

    if spot_market.market_index == QUOTE_SPOT_MARKET_INDEX {
        return Ok(());
    }

    // 1 hour EMA
    let risk_ema_price = spot_market.historical_oracle_data.last_oracle_price_twap;

    let oracle_validity = oracle_validity(
        MarketType::Spot,
        spot_market.market_index,
        risk_ema_price,
        oracle_price_data,
        validity_guard_rails,
        spot_market.get_max_confidence_interval_multiplier()?,
        false,
    )?;

    validate!(
        is_oracle_valid_for_action(oracle_validity, action)?,
        ErrorCode::InvalidOracle,
        "Invalid Oracle ({:?} vs ema={:?}) for spot market index={} and action={:?}",
        oracle_price_data,
        risk_ema_price,
        spot_market.market_index,
        action
    )?;

    Ok(())
}

fn increase_balance(
    delta: u128,
    market: &mut Market,
    balance_type: &BalanceType,
) -> NormalResult {
    match balance_type {
        BalanceType::Deposit => {
            market.deposit_balance = market.deposit_balance.safe_add(delta)?
        }
        BalanceType::Borrow => {
            market.borrow_balance = market.borrow_balance.safe_add(delta)?
        }
    }

    Ok(())
}

fn decrease_spot_balance(
    delta: u128,
    spot_market: &mut SpotMarket,
    balance_type: &SpotBalanceType,
) -> NormalResult {
    match balance_type {
        SpotBalanceType::Deposit => {
            spot_market.deposit_balance = spot_market.deposit_balance.safe_sub(delta)?
        }
        SpotBalanceType::Borrow => {
            spot_market.borrow_balance = spot_market.borrow_balance.safe_sub(delta)?
        }
    }

    Ok(())
}
