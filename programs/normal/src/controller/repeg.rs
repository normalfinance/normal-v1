use std::cmp::min;

use anchor_lang::prelude::AccountInfo;
use anchor_lang::prelude::*;
use solana_program::msg;

use crate::controller::amm::calculate_perp_market_amm_summary_stats;
use crate::controller::amm::update_spreads;
use crate::error::ErrorCode;
use crate::error::*;
use crate::load_mut;
use crate::math::amm;
use crate::math::bn;
use crate::math::casting::Cast;
use crate::math::constants::{
    K_BPS_UPDATE_SCALE,
    MAX_SQRT_K,
    QUOTE_PRECISION,
    QUOTE_SPOT_MARKET_INDEX,
};
use crate::math::cp_curve;
use crate::math::cp_curve::get_update_k_result;
use crate::math::cp_curve::UpdateKResult;
use crate::math::oracle;
use crate::math::oracle::{ is_oracle_valid_for_action, oracle_validity, NormalAction };
use crate::math::repeg;
use crate::math::safe_math::SafeMath;
use crate::math::balance::get_token_amount;

use crate::state::oracle::{ OraclePriceData, OracleSource };
use crate::state::oracle_map::OracleMap;
use crate::state::market::{ MarketStatus, Market };
use crate::state::market::Balance;
use crate::state::market::BalanceType;
use crate::state::market_map::MarketMap;
use crate::state::state::{ OracleGuardRails, State };
use crate::state::user::MarketType;
use crate::validate;

// #[cfg(test)]
// mod tests;

pub fn repeg(
    market: &mut Market,
    price_oracle: &AccountInfo,
    new_peg_candidate: u128,
    clock_slot: u64,
    oracle_guard_rails: &OracleGuardRails
) -> NormalResult<i128> {
    // for adhoc admin only repeg

    if new_peg_candidate == market.amm.peg_multiplier {
        return Err(ErrorCode::InvalidRepegRedundant);
    }
    let (terminal_price_before, _terminal_quote_reserves, _terminal_base_reserves) =
        amm::calculate_terminal_price_and_reserves(&market.amm)?;

    let (repegged_market, adjustment_cost) = repeg::adjust_peg_cost(market, new_peg_candidate)?;

    let (oracle_is_valid, direction_valid, profitability_valid, price_impact_valid) =
        repeg::calculate_repeg_validity_from_oracle_account(
            &repegged_market,
            price_oracle,
            terminal_price_before,
            clock_slot,
            oracle_guard_rails
        )?;

    // cannot repeg if oracle is invalid
    if !oracle_is_valid {
        return Err(ErrorCode::InvalidOracle);
    }

    // only push terminal in direction of oracle
    if !direction_valid {
        return Err(ErrorCode::InvalidRepegDirection);
    }

    // only push terminal up to closer edge of oracle confidence band
    if !profitability_valid {
        return Err(ErrorCode::InvalidRepegProfitability);
    }

    // only push mark up to further edge of oracle confidence band
    if !price_impact_valid {
        // todo
        // return Err(ErrorCode::InvalidRepegPriceImpact);
        return Err(ErrorCode::InvalidRepegProfitability);
    }

    // modify market's total fee change and peg change
    let cost_applied = apply_cost_to_market(market, adjustment_cost, true)?;
    if cost_applied {
        market.amm.peg_multiplier = new_peg_candidate;
    } else {
        return Err(ErrorCode::InvalidRepegProfitability);
    }

    Ok(adjustment_cost)
}

pub fn update_amms(
    market_map: &mut MarketMap,
    oracle_map: &mut OracleMap,
    state: &State,
    clock: &Clock
) -> NormalResult<bool> {
    // up to ~60k compute units (per amm) worst case
    let clock_slot = clock.slot;
    let now = clock.unix_timestamp;

    let updated = true; // todo
    for (_key, market_account_loader) in market_map.0.iter_mut() {
        let market = &mut load_mut!(market_account_loader)?;
        let oracle_price_data = &oracle_map.get_price_data(&market.amm.oracle)?;
        _update_amm(market, oracle_price_data, state, now, clock_slot)?;
    }

    Ok(updated)
}

pub fn update_amm(
    market_index: u16,
    market_map: &MarketMap,
    oracle_map: &mut OracleMap,
    state: &State,
    clock: &Clock
) -> NormalResult<i128> {
    let market = &mut market_map.get_ref_mut(&market_index)?;
    let oracle_price_data = oracle_map.get_price_data(&market.amm.oracle)?;

    let cost_of_update = _update_amm(
        market,
        oracle_price_data,
        state,
        clock.unix_timestamp,
        clock.slot
    )?;

    Ok(cost_of_update)
}

pub fn _update_amm(
    market: &mut Market,
    oracle_price_data: &OraclePriceData,
    state: &State,
    now: i64,
    clock_slot: u64
) -> NormalResult<i128> {
    if matches!(market.status, MarketStatus::Settlement | MarketStatus::Delisted) {
        return Ok(0);
    }

    let oracle_validity = oracle::oracle_validity(
        MarketType::Synthetic,
        market.market_index,
        market.amm.historical_oracle_data.last_oracle_price_twap,
        oracle_price_data,
        &state.oracle_guard_rails.validity,
        market.get_max_confidence_interval_multiplier()?,
        true
    )?;

    let mut amm_update_cost = 0;
    let mut amm_not_successfully_updated = false;
    if is_oracle_valid_for_action(oracle_validity, Some(NormalAction::UpdateAMMCurve))? {
        let curve_update_intensity = min(market.amm.curve_update_intensity, 100_u8).cast::<i128>()?;

        if curve_update_intensity > 0 {
            let (optimal_peg, fee_budget, check_lower_bound) =
                repeg::calculate_optimal_peg_and_budget(market, oracle_price_data)?;

            let (repegged_market, repegged_cost) = repeg::adjust_amm(
                market,
                optimal_peg,
                fee_budget,
                curve_update_intensity >= 100
            )?;

            let cost_applied = apply_cost_to_market(market, repegged_cost, check_lower_bound)?;
            if cost_applied {
                cp_curve::update_k(
                    market,
                    &(UpdateKResult {
                        sqrt_k: repegged_market.amm.sqrt_k,
                        base_asset_reserve: repegged_market.amm.base_asset_reserve,
                        quote_asset_reserve: repegged_market.amm.quote_asset_reserve,
                    })
                )?;
                market.amm.peg_multiplier = repegged_market.amm.peg_multiplier;
                amm_update_cost = repegged_cost;
            } else {
                msg!("amm_not_successfully_updated = true (repeg cost not applied for check_lower_bound={})", check_lower_bound);
                amm_not_successfully_updated = true;
            }
        }
    }

    let reserve_price_after = market.amm.reserve_price()?;

    if is_oracle_valid_for_action(oracle_validity, Some(NormalAction::UpdateTwap))? {
        let sanitize_clamp_denominator = market.get_sanitize_clamp_denominator()?;

        amm::update_oracle_price_twap(
            &mut market.amm,
            now,
            oracle_price_data,
            Some(reserve_price_after),
            sanitize_clamp_denominator
        )?;
    }

    if is_oracle_valid_for_action(oracle_validity, Some(NormalAction::FillOrderAmm))? {
        if !amm_not_successfully_updated {
            market.amm.last_update_slot = clock_slot;
        }
        market.amm.last_oracle_valid = true;
    } else {
        market.amm.last_oracle_valid = false;
    }

    update_spreads(market, reserve_price_after)?;

    Ok(amm_update_cost)
}

pub fn update_amm_and_check_validity(
    market: &mut Market,
    oracle_price_data: &OraclePriceData,
    state: &State,
    now: i64,
    clock_slot: u64,
    action: Option<NormalAction>
) -> NormalResult {
    _update_amm(market, oracle_price_data, state, now, clock_slot)?;

    // 1 hour EMA
    let risk_ema_price = market.amm.historical_oracle_data.last_oracle_price_twap;

    let oracle_validity = oracle_validity(
        MarketType::Synthetic,
        market.market_index,
        risk_ema_price,
        oracle_price_data,
        &state.oracle_guard_rails.validity,
        market.get_max_confidence_interval_multiplier()?,
        false
    )?;

    validate!(
        is_oracle_valid_for_action(oracle_validity, action)?,
        ErrorCode::InvalidOracle,
        "Invalid Oracle ({:?} vs ema={:?}) for market index={} and action={:?}",
        oracle_price_data,
        risk_ema_price,
        market.market_index,
        action
    )?;

    Ok(())
}

pub fn apply_cost_to_market(
    market: &mut Market,
    cost: i128,
    check_lower_bound: bool
) -> NormalResult<bool> {
    // positive cost is expense, negative cost is revenue
    // Reduce pnl to quote asset precision and take the absolute value
    if cost > 0 {
        let new_total_fee_minus_distributions =
            market.amm.total_fee_minus_distributions.safe_sub(cost)?;

        let fee_reserved_for_protocol = repeg
            ::get_total_fee_lower_bound(market)?
            // .safe_add(market.amm.total_liquidation_fee)?
            .safe_sub(market.amm.total_fee_withdrawn)?
            .cast::<i128>()?;
        // Only a portion of the protocol fees are allocated to repegging
        // This checks that the total_fee_minus_distributions does not decrease too much after repeg
        if check_lower_bound {
            if new_total_fee_minus_distributions >= fee_reserved_for_protocol {
                market.amm.total_fee_minus_distributions = new_total_fee_minus_distributions;
            } else {
                return Ok(false);
            }
        } else {
            market.amm.total_fee_minus_distributions = new_total_fee_minus_distributions;
        }
    } else {
        market.amm.total_fee_minus_distributions =
            market.amm.total_fee_minus_distributions.safe_add(cost.abs())?;
    }

    Ok(true)
}

pub fn settle_expired_market(
    market_index: u16,
    market_map: &MarketMap,
    _oracle_map: &mut OracleMap,
    _state: &State,
    clock: &Clock
) -> NormalResult {
    let now = clock.unix_timestamp;
    let market = &mut market_map.get_ref_mut(&market_index)?;

    validate!(
        market.expiry_ts != 0,
        ErrorCode::MarketSettlementAttemptOnActiveMarket,
        "Market isn't set to expire"
    )?;

    validate!(
        market.expiry_ts <= now,
        ErrorCode::MarketSettlementAttemptTooEarly,
        "Market hasn't expired yet (expiry={} > now{})",
        market.expiry_ts,
        now
    )?;

    validate!(
        market.amm.base_asset_amount_with_unsettled_lp == 0 && market.amm.user_lp_shares == 0,
        ErrorCode::MarketSettlementRequiresSettledLP,
        "Outstanding LP in market"
    )?;

    let spot_market = &mut spot_market_map.get_ref_mut(&QUOTE_SPOT_MARKET_INDEX)?;
    let fee_reserved_for_protocol = repeg
        ::get_total_fee_lower_bound(market)?
        // .safe_add(market.amm.total_liquidation_fee)?
        .safe_sub(market.amm.total_fee_withdrawn)?
        .cast::<i128>()?;
    let budget = market.amm.total_fee_minus_distributions
        .safe_sub(fee_reserved_for_protocol)?
        .max(0);

    let available_fee_pool = get_token_amount(
        market.amm.fee_pool.balance,
        spot_market
    )?
        .cast::<i128>()?
        .safe_sub(fee_reserved_for_protocol)?
        .max(0);

    let fee_pool_transfer = budget.min(available_fee_pool);

    // update_spot_balances(
    //     fee_pool_transfer.unsigned_abs(),
    //     &SpotBalanceType::Borrow,
    //     spot_market,
    //     &mut market.amm.fee_pool,
    //     false
    // )?;

    // update_spot_balances(
    //     fee_pool_transfer.unsigned_abs(),
    //     &SpotBalanceType::Deposit,
    //     spot_market,
    //     &mut market.pnl_pool,
    //     false
    // )?;

    if budget > 0 {
        let (k_scale_numerator, k_scale_denominator) = cp_curve::calculate_budgeted_k_scale(
            market,
            budget.cast()?,
            K_BPS_UPDATE_SCALE * 100,
            K_BPS_UPDATE_SCALE
        )?;

        let new_sqrt_k = bn::U192
            ::from(market.amm.sqrt_k)
            .safe_mul(bn::U192::from(k_scale_numerator))?
            .safe_div(bn::U192::from(k_scale_denominator))?
            .min(bn::U192::from(MAX_SQRT_K));

        let update_k_result = get_update_k_result(market, new_sqrt_k, true)?;

        let adjustment_cost = cp_curve::adjust_k_cost(market, &update_k_result)?;

        let cost_applied = apply_cost_to_market(market, adjustment_cost, true)?;

        validate!(cost_applied, ErrorCode::InvalidUpdateK, "Issue applying k increase on market")?;

        if cost_applied {
            cp_curve::update_k(market, &update_k_result)?;
        }
    }

    validate!(
        (10_u128).pow(spot_market.decimals) == QUOTE_PRECISION,
        ErrorCode::UnsupportedSpotMarket,
        "Only support bank.decimals == QUOTE_PRECISION"
    )?;

    let target_expiry_price = market.amm.historical_oracle_data.last_oracle_price_twap;

    crate::dlog!(target_expiry_price);

    validate!(
        target_expiry_price > 0,
        ErrorCode::MarketSettlementTargetPriceInvalid,
        "target_expiry_price <= 0 {}",
        target_expiry_price
    )?;

    let total_excess_balance2: i128 = calculate_perp_market_amm_summary_stats(
        market,
        spot_market,
        target_expiry_price
    )?;

    let pnl_pool_token_amount = get_token_amount(
        market.pnl_pool.balance,
        spot_market
    )?;

    let fee_pool_token_amount = get_token_amount(
        market.amm.fee_pool.balance,
        spot_market
    )?;

    let total_excess_balance: i128 = pnl_pool_token_amount.safe_add(fee_pool_token_amount)?.cast()?;

    crate::dlog!(market.market_index);
    crate::dlog!(total_excess_balance);

    let expiry_price = amm::calculate_expiry_price(
        &market.amm,
        target_expiry_price,
        total_excess_balance
    )?;

    market.expiry_price = expiry_price;
    market.status = MarketStatus::Settlement;

    crate::dlog!(market.expiry_price);

    Ok(())
}
