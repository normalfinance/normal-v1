use anchor_lang::prelude::{ msg, Pubkey };

use crate::bn::U192;
use crate::controller;
use crate::controller::position::update_position_and_market;
use crate::controller::position::{ get_position_index, PositionDelta };
use crate::emit;
use crate::error::{ NormalResult, ErrorCode };
use crate::get_struct_values;
use crate::math::casting::Cast;
use crate::math::cp_curve::{ get_update_k_result, update_k };
use crate::math::lp::calculate_settle_lp_metrics;
use crate::math::position::calculate_base_asset_value_with_oracle_price;
use crate::math::safe_math::SafeMath;

use crate::state::events::{ LPAction, LPRecord };
use crate::state::oracle_map::OracleMap;
use crate::state::market::Market;
use crate::state::market_map::MarketMap;
use crate::state::state::State;
use crate::state::user::Position;
use crate::state::user::User;
use crate::validate;
use anchor_lang::prelude::Account;

// #[cfg(test)]
// mod tests;

pub fn apply_lp_rebase_to_market(market: &mut Market, expo_diff: i8) -> NormalResult<()> {
    // target_base_asset_amount_per_lp is the only one that it doesnt get applied
    // thus changing the base of lp and without changing target_base_asset_amount_per_lp
    // causes an implied change

    validate!(expo_diff != 0, ErrorCode::DefaultError, "expo_diff = 0")?;

    market.amm.per_lp_base = market.amm.per_lp_base.safe_add(expo_diff)?;
    let rebase_divisor: i128 = (10_i128).pow(expo_diff.abs().cast()?);

    if expo_diff > 0 {
        market.amm.base_asset_amount_per_lp =
            market.amm.base_asset_amount_per_lp.safe_mul(rebase_divisor)?;

        market.amm.quote_asset_amount_per_lp =
            market.amm.quote_asset_amount_per_lp.safe_mul(rebase_divisor)?;

        market.amm.total_fee_earned_per_lp = market.amm.total_fee_earned_per_lp.safe_mul(
            rebase_divisor.cast()?
        )?;
    } else {
        market.amm.base_asset_amount_per_lp =
            market.amm.base_asset_amount_per_lp.safe_div(rebase_divisor)?;

        market.amm.quote_asset_amount_per_lp =
            market.amm.quote_asset_amount_per_lp.safe_div(rebase_divisor)?;

        market.amm.total_fee_earned_per_lp = market.amm.total_fee_earned_per_lp.safe_div(
            rebase_divisor.cast()?
        )?;
    }

    msg!("rebasing market_index={} per_lp_base expo_diff={}", market.market_index, expo_diff);

    crate::validation::market::validate_market(market)?;

    Ok(())
}

pub fn apply_lp_rebase_to_position(market: &Market, position: &mut Position) -> NormalResult<()> {
    let expo_diff = market.amm.per_lp_base.safe_sub(position.per_lp_base)?;

    if expo_diff > 0 {
        let rebase_divisor: i64 = (10_i64).pow(expo_diff.cast()?);

        position.last_base_asset_amount_per_lp =
            position.last_base_asset_amount_per_lp.safe_mul(rebase_divisor)?;
        position.last_quote_asset_amount_per_lp =
            position.last_quote_asset_amount_per_lp.safe_mul(rebase_divisor)?;

        msg!(
            "rebasing perp position for market_index={} per_lp_base by expo_diff={}",
            market.market_index,
            expo_diff
        );
    } else if expo_diff < 0 {
        let rebase_divisor: i64 = (10_i64).pow(expo_diff.abs().cast()?);

        position.last_base_asset_amount_per_lp =
            position.last_base_asset_amount_per_lp.safe_div(rebase_divisor)?;
        position.last_quote_asset_amount_per_lp =
            position.last_quote_asset_amount_per_lp.safe_div(rebase_divisor)?;

        msg!(
            "rebasing perp position for market_index={} per_lp_base by expo_diff={}",
            market.market_index,
            expo_diff
        );
    }

    position.per_lp_base = position.per_lp_base.safe_add(expo_diff)?;

    Ok(())
}

pub fn mint_lp_shares(
    position: &mut Position,
    market: &mut Market,
    n_shares: u64
) -> NormalResult<()> {
    let amm = market.amm;

    let (sqrt_k,) = get_struct_values!(amm, sqrt_k);

    if position.lp_shares > 0 {
        settle_lp_position(position, market)?;
    } else {
        position.last_base_asset_amount_per_lp = amm.base_asset_amount_per_lp.cast()?;
        position.last_quote_asset_amount_per_lp = amm.quote_asset_amount_per_lp.cast()?;
        position.per_lp_base = amm.per_lp_base;
    }

    // add share balance
    position.lp_shares = position.lp_shares.safe_add(n_shares)?;

    // update market state
    let new_sqrt_k = sqrt_k.safe_add(n_shares.cast()?)?;
    let new_sqrt_k_u192 = U192::from(new_sqrt_k);

    let update_k_result = get_update_k_result(market, new_sqrt_k_u192, true)?;
    update_k(market, &update_k_result)?;

    market.amm.user_lp_shares = market.amm.user_lp_shares.safe_add(n_shares.cast()?)?;

    crate::validation::market::validate_market(market)?;
    crate::validation::position::validate_position_with_market(position, market)?;

    Ok(())
}

pub fn settle_lp_position(
    position: &mut Position,
    market: &mut Market
) -> NormalResult<(PositionDelta, i64)> {
   

    apply_lp_rebase_to_position(market, position)?;

    let lp_metrics: crate::math::lp::LPMetrics = calculate_settle_lp_metrics(
        &market.amm,
        position
    )?;

    let position_delta = PositionDelta {
        base_asset_amount: lp_metrics.base_asset_amount.cast()?,
        quote_asset_amount: lp_metrics.quote_asset_amount.cast()?,
        remainder_base_asset_amount: Some(lp_metrics.remainder_base_asset_amount.cast::<i64>()?),
    };

    let pnl: i64 = update_position_and_market(position, market, &position_delta)?;

    position.last_base_asset_amount_per_lp = market.amm.base_asset_amount_per_lp.cast()?;
    position.last_quote_asset_amount_per_lp = market.amm.quote_asset_amount_per_lp.cast()?;

    crate::validation::market::validate_market(market)?;
    crate::validation::position::validate_position_with_market(position, market)?;

    Ok((position_delta, pnl))
}

pub fn settle_lp(
    user: &mut User,
    user_key: &Pubkey,
    market: &mut Market,
    now: i64
) -> NormalResult {
    if let Ok(position) = user.get_position_mut(market.market_index) {
        if position.lp_shares > 0 {
            let (position_delta, pnl) = settle_lp_position(position, market)?;

            if position_delta.base_asset_amount != 0 || position_delta.quote_asset_amount != 0 {
                crate::emit!(LPRecord {
                    ts: now,
                    action: LPAction::SettleLiquidity,
                    user: *user_key,
                    market_index: market.market_index,
                    delta_base_asset_amount: position_delta.base_asset_amount,
                    delta_quote_asset_amount: position_delta.quote_asset_amount,
                    pnl,
                    n_shares: 0,
                });
            }
        }
    }

    Ok(())
}

pub fn burn_lp_shares(
    position: &mut Position,
    market: &mut Market,
    shares_to_burn: u64,
    oracle_price: i64
) -> NormalResult<(PositionDelta, i64)> {
    // settle
    let (mut position_delta, mut pnl) = settle_lp_position(position, market)?;

    // clean up
    let unsettled_remainder = market.amm.base_asset_amount_with_unsettled_lp.safe_add(
        position.remainder_base_asset_amount.cast()?
    )?;
    if (shares_to_burn as u128) == market.amm.user_lp_shares && unsettled_remainder != 0 {
        crate::validate!(
            unsettled_remainder.unsigned_abs() <= (market.amm.order_step_size as u128),
            ErrorCode::UnableToBurnLPTokens,
            "unsettled baa on final burn too big rel to stepsize {}: {} (remainder:{})",
            market.amm.order_step_size,
            market.amm.base_asset_amount_with_unsettled_lp,
            position.remainder_base_asset_amount
        )?;

        // sub bc lps take the opposite side of the user
        position.remainder_base_asset_amount = position.remainder_base_asset_amount.safe_sub(
            unsettled_remainder.cast()?
        )?;
    }

    // update stats
    if position.remainder_base_asset_amount != 0 {
        let base_asset_amount = position.remainder_base_asset_amount as i128;

        // user closes the dust
        market.amm.base_asset_amount_with_amm =
            market.amm.base_asset_amount_with_amm.safe_sub(base_asset_amount)?;

        market.amm.base_asset_amount_with_unsettled_lp =
            market.amm.base_asset_amount_with_unsettled_lp.safe_add(base_asset_amount)?;

        let dust_base_asset_value = calculate_base_asset_value_with_oracle_price(
            base_asset_amount,
            oracle_price
        )?.safe_add(1)?; // round up

        let dust_burn_position_delta = PositionDelta {
            base_asset_amount: 0,
            quote_asset_amount: -dust_base_asset_value.cast()?,
            remainder_base_asset_amount: Some(-position.remainder_base_asset_amount.cast()?),
        };

        update_position_and_market(position, market, &dust_burn_position_delta)?;

        msg!(
            "perp {} remainder_base_asset_amount burn fee= {}",
            position.market_index,
            dust_base_asset_value
        );

        position_delta.quote_asset_amount = position_delta.quote_asset_amount.safe_sub(
            dust_base_asset_value.cast()?
        )?;
        pnl = pnl.safe_sub(dust_base_asset_value.cast()?)?;
    }

    // update last_ metrics
    position.last_base_asset_amount_per_lp = market.amm.base_asset_amount_per_lp.cast()?;
    position.last_quote_asset_amount_per_lp = market.amm.quote_asset_amount_per_lp.cast()?;

    // burn shares
    position.lp_shares = position.lp_shares.safe_sub(shares_to_burn)?;

    market.amm.user_lp_shares = market.amm.user_lp_shares.safe_sub(shares_to_burn.cast()?)?;

    // update market state
    let new_sqrt_k = market.amm.sqrt_k.safe_sub(shares_to_burn.cast()?)?;
    let new_sqrt_k_u192 = U192::from(new_sqrt_k);

    let update_k_result = get_update_k_result(market, new_sqrt_k_u192, false)?;
    update_k(market, &update_k_result)?;

    crate::validation::market::validate_market(market)?;
    crate::validation::position::validate_position_with_market(position, market)?;

    Ok((position_delta, pnl))
}

pub fn remove_lp_shares(
    market_map: MarketMap,
    oracle_map: &mut OracleMap,
    state: &Account<State>,
    user: &mut std::cell::RefMut<User>,
    user_key: Pubkey,
    shares_to_burn: u64,
    market_index: u16,
    now: i64
) -> NormalResult<()> {
    let position_index = get_position_index(&user.positions, market_index)?;

    // standardize n shares to burn
    // account for issue where lp shares are smaller than step size
    let shares_to_burn = if user.positions[position_index].lp_shares == shares_to_burn {
        shares_to_burn
    } else {
        let market = market_map.get_ref(&market_index)?;
        crate::math::orders
            ::standardize_base_asset_amount(shares_to_burn.cast()?, market.amm.order_step_size)?
            .cast()?
    };

    if shares_to_burn == 0 {
        return Ok(());
    }

    let mut market = market_map.get_ref_mut(&market_index)?;

    let time_since_last_add_liquidity = now.safe_sub(user.last_add_lp_shares_ts)?;

    validate!(
        time_since_last_add_liquidity >= state.lp_cooldown_time.cast()?,
        ErrorCode::TryingToRemoveLiquidityTooFast
    )?;

    // controller::funding::settle_funding_payment(user, &user_key, &mut market, now)?;

    let position = &mut user.positions[position_index];

    validate!(position.lp_shares >= shares_to_burn, ErrorCode::InsufficientLPTokens)?;

    let oracle_price = oracle_map.get_price_data(&market.amm.oracle)?.price;
    let (position_delta, pnl) = burn_lp_shares(
        position,
        &mut market,
        shares_to_burn,
        oracle_price
    )?;

    emit!(LPRecord {
        ts: now,
        action: LPAction::RemoveLiquidity,
        user: user_key,
        n_shares: shares_to_burn,
        market_index,
        delta_base_asset_amount: position_delta.base_asset_amount,
        delta_quote_asset_amount: position_delta.quote_asset_amount,
        pnl,
    });

    Ok(())
}
