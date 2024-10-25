use std::convert::identity;
use std::mem::size_of;

use anchor_lang::prelude::*;
use anchor_spl::token::Token;
use anchor_spl::token_2022::Token2022;
use anchor_spl::token_interface::{ Mint, TokenAccount, TokenInterface };
use pyth_solana_receiver_sdk::cpi::accounts::InitPriceUpdate;
use pyth_solana_receiver_sdk::program::PythSolanaReceiver;

use solana_program::msg;

use crate::controller::token::close_vault;
use crate::error::ErrorCode;
use crate::ids::admin_hot_wallet;
use crate::instructions::constraints::*;
use crate::instructions::optional_accounts::{ load_maps, AccountMaps };
use crate::math::casting::Cast;
use crate::math::constants::{
    MAX_SQRT_K,
    MAX_UPDATE_K_PRICE_CHANGE,
    PERCENTAGE_PRECISION,
    QUOTE_SPOT_MARKET_INDEX,
    TWENTY_FOUR_HOUR,
    THIRTEEN_DAY,
};
use crate::math::cp_curve::get_update_k_result;
use crate::math::orders::is_multiple_of_step_size;
use crate::math::repeg::get_total_fee_lower_bound;
use crate::math::safe_math::SafeMath;
use crate::math::spot_balance::get_token_amount;
use crate::math::{ amm, bn };
use crate::optional_accounts::get_token_mint;
use crate::state::events::CurveRecord;
use crate::state::oracle::get_sb_on_demand_price;
use crate::state::oracle::{
    get_oracle_price,
    get_pyth_price,
    get_switchboard_price,
    HistoricalIndexData,
    HistoricalOracleData,
    OraclePriceData,
    OracleSource,
};
use crate::state::oracle_map::OracleMap;
use crate::state::paused_operations::{ Operation };
use crate::state::market::{ SyntheticTier, AssetType, MarketStatus, Market, PoolBalance };
use crate::state::market_map::get_writable_market_set;
// use crate::state::market::{
//     SyntheticTier, SpotBalanceType, SpotFulfillmentConfigStatus, SpotMarket,
// };
use crate::state::amm::AMM;
use crate::state::state::{ ExchangeStatus, FeeStructure, OracleGuardRails, State };
use crate::state::traits::Size;
use crate::state::user::{ User, UserStats };
use crate::state::insurance::InsuranceFund;
use crate::validate;
use crate::validation::fee_structure::validate_fee_structure;
use crate::validation::market::validate_market;
use crate::{ controller, QUOTE_PRECISION_I64 };
use crate::{ get_then_update_id, EPOCH_DURATION };
use crate::{ load, FEE_ADJUSTMENT_MAX };
use crate::{ load_mut, PTYH_PRICE_FEED_SEED_PREFIX };
use crate::{ math, safe_decrement, safe_increment };
use crate::{ math_error, SPOT_BALANCE_PRECISION };

pub fn handle_initialize(ctx: Context<Initialize>) -> Result<()> {
    let (normal_signer, normal_signer_nonce) = Pubkey::find_program_address(
        &[b"normal_signer".as_ref()],
        ctx.program_id
    );

    **ctx.accounts.state = State {
        admin: *ctx.accounts.admin.key,
        exchange_status: ExchangeStatus::active(),
        whitelist_mint: Pubkey::default(),
        discount_mint: Pubkey::default(),
        oracle_guard_rails: OracleGuardRails::default(),
        number_of_authorities: 0,
        number_of_sub_accounts: 0,
        number_of_markets: 0,
        min_auction_duration: 10,
        default_market_order_time_in_force: 60,
        default_auction_duration: 10,
        settlement_duration: 0, // extra duration after market expiry to allow settlement
        signer: normal_signer,
        signer_nonce: normal_signer_nonce,
        fee_structure: FeeStructure::default(),
        lp_cooldown_time: 0,
        max_number_of_sub_accounts: 0,
        max_initialize_user_fee: 0,
        padding: [0; 10],
    };

    Ok(())
}

#[access_control(has_been_approved(&ctx.accounts.governance_program))]
pub fn handle_initialize_market(
    ctx: Context<InitializeMarket>,
    oracle_source: OracleSource,
    active_status: bool,
    synthetic_tier: SyntheticTier,
    order_tick_size: u64,
    order_step_size: u64,
    name: [u8; 32],
    // perp
    amm_base_asset_reserve: u128,
    amm_quote_asset_reserve: u128,
    amm_periodicity: i64,
    amm_peg_multiplier: u128,
    active_status: bool,
    base_spread: u32,
    max_spread: u32,
    max_open_interest: u128,
    quote_max_insurance: u64,
    order_step_size: u64,
    order_tick_size: u64,
    min_order_size: u64,
    concentration_coef_scale: u128,
    curve_update_intensity: u8,
    amm_jit_intensity: u8
) -> Result<()> {
    let state = &mut ctx.accounts.state;
    let market_pubkey = ctx.accounts.market.key();

    // protocol must be authority of collateral vault
    if ctx.accounts.market_vault.owner != state.signer {
        return Err(ErrorCode::InvalidMarketAuthority.into());
    }

    let market_index = get_then_update_id!(state, number_of_markets);

    msg!("initializing market {}", market_index);

    if oracle_source == OracleSource::QuoteAsset {
        // catches inconsistent parameters
        validate!(
            ctx.accounts.oracle.key == &Pubkey::default(),
            ErrorCode::InvalidSpotMarketInitialization,
            "For OracleSource::QuoteAsset, oracle must be default public key"
        )?;

        validate!(
            market_index == QUOTE_SPOT_MARKET_INDEX,
            ErrorCode::InvalidSpotMarketInitialization,
            "For OracleSource::QuoteAsset, market_index must be QUOTE_SPOT_MARKET_INDEX"
        )?;
    } else {
        OracleMap::validate_oracle_account_info(&ctx.accounts.oracle)?;
    }

    let oracle_price_data = get_oracle_price(
        &oracle_source,
        &ctx.accounts.oracle,
        Clock::get()?.unix_timestamp.cast()?
    );

    let (historical_oracle_data_default, historical_index_data_default) = if
        market_index == QUOTE_SPOT_MARKET_INDEX
    {
        validate!(
            ctx.accounts.oracle.key == &Pubkey::default(),
            ErrorCode::InvalidSpotMarketInitialization,
            "For quote asset spot market, oracle must be default public key"
        )?;

        validate!(
            oracle_source == OracleSource::QuoteAsset,
            ErrorCode::InvalidSpotMarketInitialization,
            "For quote asset spot market, oracle source must be QuoteAsset"
        )?;

        validate!(
            ctx.accounts.market_mint.decimals == 6,
            ErrorCode::InvalidSpotMarketInitialization,
            "For quote asset spot market, mint decimals must be 6"
        )?;

        (HistoricalOracleData::default_quote_oracle(), HistoricalIndexData::default_quote_oracle())
    } else {
        validate!(
            ctx.accounts.market_mint.decimals >= 6,
            ErrorCode::InvalidSpotMarketInitialization,
            "Mint decimals must be greater than or equal to 6"
        )?;

        validate!(
            oracle_price_data.is_ok(),
            ErrorCode::InvalidSpotMarketInitialization,
            "Unable to read oracle price for {}",
            ctx.accounts.oracle.key
        )?;

        (
            HistoricalOracleData::default_with_current_oracle(oracle_price_data?),
            HistoricalIndexData::default_with_current_oracle(oracle_price_data?)?,
        )
    };

    let market = &mut ctx.accounts.market.load_init()?;
    let clock = Clock::get()?;
    let now = clock.unix_timestamp.cast().or(Err(ErrorCode::UnableToCastUnixTime))?;

    let decimals = ctx.accounts.market_mint.decimals.cast::<u32>()?;

    let token_program = if ctx.accounts.token_program.key() == Token2022::id() {
        1_u8
    } else if ctx.accounts.token_program.key() == Token::id() {
        0_u8
    } else {
        msg!("unexpected program {:?}", ctx.accounts.token_program.key());
        return Err(ErrorCode::DefaultError.into());
    };

    **market = Market {
        market_index: market_index,
        pubkey: market_pubkey,
        status: if active_status {
            MarketStatus::Active
        } else {
            MarketStatus::Initialized
        },
        name,
        synthetic_tier,
        expiry_ts: 0,
        oracle: ctx.accounts.oracle.key(),
        oracle_source,
        historical_oracle_data: historical_oracle_data_default,
        historical_index_data: historical_index_data_default,
        mint: ctx.accounts.market_mint.key(),
        vault: *ctx.accounts.market_vault.to_account_info().key,
        decimals,
        // total_social_loss: 0,
        // total_quote_social_loss: 0,
        // last_twap_ts: now,
        order_step_size,
        order_tick_size,
        min_order_size: order_step_size,
        max_position_size: 0,
        next_fill_record_id: 1,
        fee_pool: PoolBalance::default(), // in quote asset
        total_fee: 0,
        paused_operations: 0,
        fee_adjustment: 0,
        number_of_users_with_base: 0,
        number_of_users: 0,

        token_program,
        padding: [0; 41],

        /// Insurance
        insurance_fund: ctx.accounts.insurance_fund.key(),
        insurance_claim: InsuranceClaim {
            quote_max_insurance,
            ..InsuranceClaim::default()
        },

        amm: AMM {
            // TODO: are these inits correct?
            token: 0,
            token_mint: 0,
            oracle: *ctx.accounts.oracle.key,
            oracle_source,
            base_asset_reserve: amm_base_asset_reserve,
            quote_asset_reserve: amm_quote_asset_reserve,
            terminal_quote_asset_reserve: amm_quote_asset_reserve,
            ask_base_asset_reserve: amm_base_asset_reserve,
            ask_quote_asset_reserve: amm_quote_asset_reserve,
            bid_base_asset_reserve: amm_base_asset_reserve,
            bid_quote_asset_reserve: amm_quote_asset_reserve,
            total_social_loss: 0,
            last_mark_price_twap: init_reserve_price,
            last_mark_price_twap_5min: init_reserve_price,
            last_mark_price_twap_ts: now,
            sqrt_k: amm_base_asset_reserve,
            concentration_coef,
            min_base_asset_reserve,
            max_base_asset_reserve,
            peg_multiplier: amm_peg_multiplier,
            total_fee: 0,
            total_fee_withdrawn: 0,
            total_fee_minus_distributions: 0,
            total_mm_fee: 0,
            total_exchange_fee: 0,
            historical_oracle_data: HistoricalOracleData {
                last_oracle_price: oracle_price,
                last_oracle_delay: oracle_delay,
                last_oracle_price_twap,
                last_oracle_price_twap_5min: oracle_price,
                last_oracle_price_twap_ts: now,
                ..HistoricalOracleData::default()
            },
            last_oracle_normalised_price: oracle_price,
            last_oracle_conf_pct: 0,
            last_oracle_reserve_price_spread_pct: 0, // todo
            order_step_size,
            order_tick_size,
            min_order_size,
            max_position_size: 0,
            max_slippage_ratio: 50, // ~2%
            max_fill_reserve_fraction: 100, // moves price ~2%
            base_spread,
            long_spread: 0,
            short_spread: 0,
            max_spread,
            last_bid_price_twap: init_reserve_price,
            last_ask_price_twap: init_reserve_price,
            base_asset_amount_with_amm: 0,
            base_asset_amount_long: 0,
            base_asset_amount_short: 0,
            quote_asset_amount: 0,
            quote_entry_amount_long: 0,
            quote_entry_amount_short: 0,
            quote_break_even_amount_long: 0,
            quote_break_even_amount_short: 0,
            max_open_interest,
            mark_std: 0,
            oracle_std: 0,
            volume_24h: 0,
            long_intensity_count: 0,
            long_intensity_volume: 0,
            short_intensity_count: 0,
            short_intensity_volume: 0,
            last_trade_ts: now,
            curve_update_intensity,
            fee_pool: PoolBalance::default(),
            base_asset_amount_per_lp: 0,
            quote_asset_amount_per_lp: 0,
            last_update_slot: clock_slot,

            // lp stuff
            base_asset_amount_with_unsettled_lp: 0,
            user_lp_shares: 0,
            amm_jit_intensity,

            last_oracle_valid: false,
            target_base_asset_amount_per_lp: 0,
            per_lp_base: 0,
            padding1: 0,
            padding2: 0,
            total_fee_earned_per_lp: 0,
            quote_asset_amount_with_unsettled_lp: 0,
            reference_price_offset: 0,
            padding: [0; 12],
        },
    };

    controller::amm::initialize_synthetic_token(market);

    Ok(())
}

pub fn handle_delete_initialized_market(
    ctx: Context<DeleteInitializedMarket>,
    market_index: u16
) -> Result<()> {
    let market = ctx.accounts.market.load()?;
    msg!("market {}", market.market_index);
    let state = &mut ctx.accounts.state;

    // to preserve all protocol invariants, can only remove the last market if it hasn't been "activated"

    validate!(
        state.number_of_markets - 1 == market_index,
        ErrorCode::InvalidMarketAccountforDeletion,
        "state.number_of_markets={} != market_index={}",
        state.number_of_markets,
        market_index
    )?;
    validate!(
        market.status == MarketStatus::Initialized,
        ErrorCode::InvalidMarketAccountforDeletion,
        "market.status != Initialized"
    )?;
    validate!(
        market.deposit_balance == 0,
        ErrorCode::InvalidMarketAccountforDeletion,
        "market.number_of_users={} != 0",
        market.deposit_balance
    )?;
    validate!(
        market.borrow_balance == 0,
        ErrorCode::InvalidMarketAccountforDeletion,
        "market.borrow_balance={} != 0",
        market.borrow_balance
    )?;
    validate!(
        market.market_index == market_index,
        ErrorCode::InvalidMarketAccountforDeletion,
        "market_index={} != market.market_index={}",
        market_index,
        market.market_index
    )?;

    safe_decrement!(state.number_of_markets, 1);

    drop(market);

    validate!(
        ctx.accounts.market_vault.amount == 0,
        ErrorCode::InvalidMarketAccountforDeletion,
        "ctx.accounts.market_vault.amount={}",
        ctx.accounts.market_vault.amount
    )?;

    close_vault(
        &ctx.accounts.token_program,
        &ctx.accounts.market_vault,
        &ctx.accounts.admin.to_account_info(),
        &ctx.accounts.normal_signer,
        state.signer_nonce
    )?;

    Ok(())
}

#[access_control(market_valid(&ctx.accounts.market))]
pub fn handle_update_market_oracle(
    ctx: Context<AdminUpdateMarketOracle>,
    oracle: Pubkey,
    oracle_source: OracleSource
) -> Result<()> {
    let market = &mut load_mut!(ctx.accounts.market)?;
    msg!("updating market {} oracle", market.market_index);
    let clock = Clock::get()?;

    OracleMap::validate_oracle_account_info(&ctx.accounts.oracle)?;

    validate!(
        ctx.accounts.oracle.key == &oracle,
        ErrorCode::DefaultError,
        "oracle account info ({:?}) and ix data ({:?}) must match",
        ctx.accounts.oracle.key,
        oracle
    )?;

    // Verify oracle is readable
    let OraclePriceData {
        price: _oracle_price,
        delay: _oracle_delay,
        ..
    } = get_oracle_price(&oracle_source, &ctx.accounts.oracle, clock.slot)?;

    msg!("market.oracle {:?} -> {:?}", market.oracle, oracle);

    msg!("market.oracle_source {:?} -> {:?}", market.oracle_source, oracle_source);

    market.oracle = oracle;
    market.oracle_source = oracle_source;
    Ok(())
}

#[access_control(market_valid(&ctx.accounts.market))]
pub fn handle_update_market_expiry(ctx: Context<AdminUpdateMarket>, expiry_ts: i64) -> Result<()> {
    let market = &mut load_mut!(ctx.accounts.market)?;
    msg!("updating market {} expiry", market.market_index);
    let now = Clock::get()?.unix_timestamp;

    validate!(
        now < expiry_ts,
        ErrorCode::DefaultError,
        "Market expiry ts must later than current clock timestamp"
    )?;

    msg!("market.status {:?} -> {:?}", market.status, MarketStatus::ReduceOnly);
    msg!("market.expiry_ts {} -> {}", market.expiry_ts, expiry_ts);

    // automatically enter reduce only
    market.status = MarketStatus::ReduceOnly;
    market.expiry_ts = expiry_ts;

    Ok(())
}

#[access_control(market_valid(&ctx.accounts.market))]
pub fn handle_move_amm_price(
    ctx: Context<AdminUpdateMarket>,
    base_asset_reserve: u128,
    quote_asset_reserve: u128,
    sqrt_k: u128
) -> Result<()> {
    let market = &mut load_mut!(ctx.accounts.market)?;

    msg!("moving amm price for market {}", market.market_index);

    let base_asset_reserve_before = market.amm.base_asset_reserve;
    let quote_asset_reserve_before = market.amm.quote_asset_reserve;
    let sqrt_k_before = market.amm.sqrt_k;
    let max_base_asset_reserve_before = market.amm.max_base_asset_reserve;
    let min_base_asset_reserve_before = market.amm.min_base_asset_reserve;

    controller::amm::move_price(market, base_asset_reserve, quote_asset_reserve, sqrt_k)?;
    validate_market(market)?;

    let base_asset_reserve_after = market.amm.base_asset_reserve;
    let quote_asset_reserve_after = market.amm.quote_asset_reserve;
    let sqrt_k_after = market.amm.sqrt_k;
    let max_base_asset_reserve_after = market.amm.max_base_asset_reserve;
    let min_base_asset_reserve_after = market.amm.min_base_asset_reserve;

    msg!("base_asset_reserve {} -> {}", base_asset_reserve_before, base_asset_reserve_after);

    msg!("quote_asset_reserve {} -> {}", quote_asset_reserve_before, quote_asset_reserve_after);

    msg!("sqrt_k {} -> {}", sqrt_k_before, sqrt_k_after);

    msg!(
        "max_base_asset_reserve {} -> {}",
        max_base_asset_reserve_before,
        max_base_asset_reserve_after
    );

    msg!(
        "min_base_asset_reserve {} -> {}",
        min_base_asset_reserve_before,
        min_base_asset_reserve_after
    );

    Ok(())
}

#[access_control(market_valid(&ctx.accounts.market))]
pub fn handle_recenter_market_amm(
    ctx: Context<AdminUpdateMarket>,
    peg_multiplier: u128,
    sqrt_k: u128
) -> Result<()> {
    let market = &mut load_mut!(ctx.accounts.market)?;

    msg!("recentering amm for market {}", market.market_index);

    let base_asset_reserve_before = market.amm.base_asset_reserve;
    let quote_asset_reserve_before = market.amm.quote_asset_reserve;
    let sqrt_k_before = market.amm.sqrt_k;
    let peg_multiplier_before = market.amm.peg_multiplier;
    let max_base_asset_reserve_before = market.amm.max_base_asset_reserve;
    let min_base_asset_reserve_before = market.amm.min_base_asset_reserve;

    controller::amm::recenter_market_amm(market, peg_multiplier, sqrt_k)?;
    validate_market(market)?;

    let base_asset_reserve_after = market.amm.base_asset_reserve;
    let quote_asset_reserve_after = market.amm.quote_asset_reserve;
    let sqrt_k_after = market.amm.sqrt_k;
    let peg_multiplier_after = market.amm.peg_multiplier;
    let max_base_asset_reserve_after = market.amm.max_base_asset_reserve;
    let min_base_asset_reserve_after = market.amm.min_base_asset_reserve;

    msg!("base_asset_reserve {} -> {}", base_asset_reserve_before, base_asset_reserve_after);

    msg!("quote_asset_reserve {} -> {}", quote_asset_reserve_before, quote_asset_reserve_after);

    msg!("sqrt_k {} -> {}", sqrt_k_before, sqrt_k_after);

    msg!("peg_multiplier {} -> {}", peg_multiplier_before, peg_multiplier_after);

    msg!(
        "max_base_asset_reserve {} -> {}",
        max_base_asset_reserve_before,
        max_base_asset_reserve_after
    );

    msg!(
        "min_base_asset_reserve {} -> {}",
        min_base_asset_reserve_before,
        min_base_asset_reserve_after
    );

    Ok(())
}

#[derive(Debug, Clone, Copy, AnchorSerialize, AnchorDeserialize, PartialEq, Eq)]
pub struct UpdateMarketSummaryStatsParams {
    // new aggregate unsettled user stats
    pub quote_asset_amount_with_unsettled_lp: Option<i64>,
    pub update_amm_summary_stats: Option<bool>,
}

#[access_control(
    market_valid(&ctx.accounts.market)
    valid_oracle_for_market(&ctx.accounts.oracle, &ctx.accounts.market)
)]
pub fn handle_update_market_amm_summary_stats(
    ctx: Context<AdminUpdateMarketAmmSummaryStats>,
    params: UpdateMarketSummaryStatsParams
) -> Result<()> {
    let market = &mut load_mut!(ctx.accounts.market)?;

    msg!("updating amm summary stats for market {}", market.market_index);

    let clock = Clock::get()?;
    let price_oracle = &ctx.accounts.oracle;

    let OraclePriceData { price: oracle_price, .. } = get_oracle_price(
        &market.amm.oracle_source,
        price_oracle,
        clock.slot
    )?;

    if let Some(quote_asset_amount_with_unsettled_lp) = params.quote_asset_amount_with_unsettled_lp {
        msg!(
            "quote_asset_amount_with_unsettled_lp {} -> {}",
            market.amm.quote_asset_amount_with_unsettled_lp,
            quote_asset_amount_with_unsettled_lp
        );
        market.amm.quote_asset_amount_with_unsettled_lp = quote_asset_amount_with_unsettled_lp;
    }

    if params.update_amm_summary_stats == Some(true) {
        let new_total_fee_minus_distributions = controller::amm::calculate_market_amm_summary_stats(
            market,
            oracle_price
        )?;

        msg!("updating amm summary stats for market index = {}", market.market_index);

        msg!(
            "total_fee_minus_distributions: {:?} -> {:?}",
            market.amm.total_fee_minus_distributions,
            new_total_fee_minus_distributions
        );

        let fee_difference = new_total_fee_minus_distributions.safe_sub(
            market.amm.total_fee_minus_distributions
        )?;

        msg!(
            "market.amm.total_fee: {} -> {}",
            market.amm.total_fee,
            market.amm.total_fee.saturating_add(fee_difference)
        );

        msg!(
            "market.amm.total_mm_fee: {} -> {}",
            market.amm.total_mm_fee,
            market.amm.total_mm_fee.saturating_add(fee_difference)
        );

        market.amm.total_fee = market.amm.total_fee.saturating_add(fee_difference);
        market.amm.total_mm_fee = market.amm.total_mm_fee.saturating_add(fee_difference);
        market.amm.total_fee_minus_distributions = new_total_fee_minus_distributions;
    }
    validate_market(market)?;

    Ok(())
}

#[access_control(market_valid(&ctx.accounts.market))]
pub fn handle_settle_expired_market_pools_to_revenue_pool(
    ctx: Context<SettleExpiredMarketPoolsToRevenuePool>
) -> Result<()> {
    let market = &mut load_mut!(ctx.accounts.market)?;
    let state = &ctx.accounts.state;

    msg!(
        "settling expired market pools to revenue pool for perp market {}",
        market.market_index
    );

    let clock = Clock::get()?;
    let now = clock.unix_timestamp;

    // controller::spot_balance::update_spot_market_cumulative_interest(spot_market, None, now)?;

    validate!(
        spot_market.market_index == QUOTE_SPOT_MARKET_INDEX,
        ErrorCode::DefaultError,
        "spot_market must be perp market's quote asset"
    )?;

    validate!(
        market.status == MarketStatus::Settlement,
        ErrorCode::DefaultError,
        "Market must in Settlement"
    )?;

    validate!(
        market.amm.base_asset_amount_long == 0 &&
            market.amm.base_asset_amount_short == 0 &&
            market.number_of_users_with_base == 0,
        ErrorCode::DefaultError,
        "outstanding base_asset_amounts must be balanced {} {} {}",
        market.amm.base_asset_amount_long,
        market.amm.base_asset_amount_short,
        market.number_of_users_with_base
    )?;

    validate!(
        math::amm::calculate_net_user_cost_basis(&market.amm)? == 0,
        ErrorCode::DefaultError,
        "outstanding quote_asset_amounts must be balanced"
    )?;

    // block when settlement_duration is default/unconfigured
    validate!(
        state.settlement_duration != 0,
        ErrorCode::DefaultError,
        "invalid state.settlement_duration (is 0)"
    )?;

    let escrow_period_before_transfer = if state.settlement_duration > 1 {
        // minimum of TWENTY_FOUR_HOUR to examine settlement process
        TWENTY_FOUR_HOUR.safe_add(state.settlement_duration.cast()?)?.safe_sub(1)?
    } else {
        // for testing / expediting if settlement_duration not default but 1
        state.settlement_duration.cast::<i64>()?
    };

    validate!(
        now > market.expiry_ts.safe_add(escrow_period_before_transfer)?,
        ErrorCode::DefaultError,
        "must be escrow_period_before_transfer={} after market.expiry_ts",
        escrow_period_before_transfer
    )?;

    let fee_pool_token_amount = get_token_amount(
        market.amm.fee_pool.balance,
        spot_market
    )?;
    let pnl_pool_token_amount = get_token_amount(
        market.pnl_pool.balance,
        spot_market
    )?;

    // TODO: update
    // controller::spot_balance::update_spot_balances(
    //     fee_pool_token_amount,
    //     &SpotBalanceType::Borrow,
    //     spot_market,
    //     &mut market.amm.fee_pool,
    //     false
    // )?;

    // controller::spot_balance::update_spot_balances(
    //     pnl_pool_token_amount,
    //     &SpotBalanceType::Borrow,
    //     spot_market,
    //     &mut market.pnl_pool,
    //     false
    // )?;

    // controller::spot_balance::update_revenue_pool_balances(
    //     pnl_pool_token_amount.safe_add(fee_pool_token_amount)?,
    //     &SpotBalanceType::Deposit,
    //     spot_market
    // )?;

    // math::spot_withdraw::validate_spot_balances(spot_market)?;

    market.status = MarketStatus::Delisted;

    Ok(())
}

#[access_control(
    market_valid(&ctx.accounts.market)
    valid_oracle_for_market(&ctx.accounts.oracle, &ctx.accounts.market)
)]
pub fn handle_repeg_amm_curve(ctx: Context<RepegCurve>, new_peg_candidate: u128) -> Result<()> {
    let clock = Clock::get()?;
    let now = clock.unix_timestamp;
    let clock_slot = clock.slot;

    let market = &mut load_mut!(ctx.accounts.market)?;
    msg!("repegging amm curve for market {}", market.market_index);

    let price_oracle = &ctx.accounts.oracle;
    let OraclePriceData { price: oracle_price, .. } = get_oracle_price(
        &market.amm.oracle_source,
        price_oracle,
        clock.slot
    )?;

    let peg_multiplier_before = market.amm.peg_multiplier;
    let base_asset_reserve_before = market.amm.base_asset_reserve;
    let quote_asset_reserve_before = market.amm.quote_asset_reserve;
    let sqrt_k_before = market.amm.sqrt_k;

    let oracle_validity_rails = &ctx.accounts.state.oracle_guard_rails;

    let adjustment_cost = controller::repeg::repeg(
        market,
        price_oracle,
        new_peg_candidate,
        clock_slot,
        oracle_validity_rails
    )?;

    let peg_multiplier_after = market.amm.peg_multiplier;
    let base_asset_reserve_after = market.amm.base_asset_reserve;
    let quote_asset_reserve_after = market.amm.quote_asset_reserve;
    let sqrt_k_after = market.amm.sqrt_k;

    msg!("market.amm.peg_multiplier {} -> {}", peg_multiplier_before, peg_multiplier_after);

    msg!(
        "market.amm.base_asset_reserve {} -> {}",
        base_asset_reserve_before,
        base_asset_reserve_after
    );

    msg!(
        "market.amm.quote_asset_reserve {} -> {}",
        quote_asset_reserve_before,
        quote_asset_reserve_after
    );

    msg!("market.amm.sqrt_k {} -> {}", sqrt_k_before, sqrt_k_after);

    emit!(CurveRecord {
        ts: now,
        record_id: get_then_update_id!(market, next_curve_record_id),
        market_index: market.market_index,
        peg_multiplier_before,
        base_asset_reserve_before,
        quote_asset_reserve_before,
        sqrt_k_before,
        peg_multiplier_after,
        base_asset_reserve_after,
        quote_asset_reserve_after,
        sqrt_k_after,
        base_asset_amount_long: market.amm.base_asset_amount_long.unsigned_abs(),
        base_asset_amount_short: market.amm.base_asset_amount_short.unsigned_abs(),
        base_asset_amount_with_amm: market.amm.base_asset_amount_with_amm,
        number_of_users: market.number_of_users,
        total_fee: market.amm.total_fee,
        total_fee_minus_distributions: market.amm.total_fee_minus_distributions,
        adjustment_cost,
        oracle_price,
        fill_record: 0,
    });

    Ok(())
}

#[access_control(
    market_valid(&ctx.accounts.market)
    valid_oracle_for_market(&ctx.accounts.oracle, &ctx.accounts.market)
)]
pub fn handle_update_amm_oracle_twap(ctx: Context<RepegCurve>) -> Result<()> {
    // allow update to amm's oracle twap iff price gap is reduced and thus more tame funding
    // otherwise if oracle error or funding flip: set oracle twap to mark twap (0 gap)

    let clock = Clock::get()?;
    let now = clock.unix_timestamp;

    let market = &mut load_mut!(ctx.accounts.market)?;
    msg!("updating amm oracle twap for perp market {}", market.market_index);
    let price_oracle = &ctx.accounts.oracle;
    let oracle_twap = market.amm.get_oracle_twap(price_oracle, clock.slot)?;

    if let Some(oracle_twap) = oracle_twap {
        let oracle_mark_gap_before = market.amm.last_mark_price_twap
            .cast::<i64>()?
            .safe_sub(market.amm.historical_oracle_data.last_oracle_price_twap)?;

        let oracle_mark_gap_after = market.amm.last_mark_price_twap
            .cast::<i64>()?
            .safe_sub(oracle_twap)?;

        if
            (oracle_mark_gap_after > 0 && oracle_mark_gap_before < 0) ||
            (oracle_mark_gap_after < 0 && oracle_mark_gap_before > 0)
        {
            msg!(
                "market.amm.historical_oracle_data.last_oracle_price_twap {} -> {}",
                market.amm.historical_oracle_data.last_oracle_price_twap,
                market.amm.last_mark_price_twap.cast::<i64>()?
            );
            msg!(
                "market.amm.historical_oracle_data.last_oracle_price_twap_ts {} -> {}",
                market.amm.historical_oracle_data.last_oracle_price_twap_ts,
                now
            );
            market.amm.historical_oracle_data.last_oracle_price_twap =
                market.amm.last_mark_price_twap.cast::<i64>()?;
            market.amm.historical_oracle_data.last_oracle_price_twap_ts = now;
        } else if oracle_mark_gap_after.unsigned_abs() <= oracle_mark_gap_before.unsigned_abs() {
            msg!(
                "market.amm.historical_oracle_data.last_oracle_price_twap {} -> {}",
                market.amm.historical_oracle_data.last_oracle_price_twap,
                oracle_twap
            );
            msg!(
                "market.amm.historical_oracle_data.last_oracle_price_twap_ts {} -> {}",
                market.amm.historical_oracle_data.last_oracle_price_twap_ts,
                now
            );
            market.amm.historical_oracle_data.last_oracle_price_twap = oracle_twap;
            market.amm.historical_oracle_data.last_oracle_price_twap_ts = now;
        } else {
            return Err(ErrorCode::PriceBandsBreached.into());
        }
    } else {
        return Err(ErrorCode::InvalidOracle.into());
    }

    Ok(())
}

#[access_control(
    market_valid(&ctx.accounts.market)
    valid_oracle_for_market(&ctx.accounts.oracle, &ctx.accounts.market)
)]
pub fn handle_update_k(ctx: Context<AdminUpdateK>, sqrt_k: u128) -> Result<()> {
    let clock = Clock::get()?;
    let now = clock.unix_timestamp;

    let market = &mut load_mut!(ctx.accounts.market)?;

    msg!("updating k for perp market {}", market.market_index);
    let base_asset_amount_long = market.amm.base_asset_amount_long.unsigned_abs();
    let base_asset_amount_short = market.amm.base_asset_amount_short.unsigned_abs();
    let base_asset_amount_with_amm = market.amm.base_asset_amount_with_amm;
    let number_of_users = market.number_of_users_with_base;

    let price_before = math::amm::calculate_price(
        market.amm.quote_asset_reserve,
        market.amm.base_asset_reserve,
        market.amm.peg_multiplier
    )?;

    let peg_multiplier_before = market.amm.peg_multiplier;
    let base_asset_reserve_before = market.amm.base_asset_reserve;
    let quote_asset_reserve_before = market.amm.quote_asset_reserve;
    let sqrt_k_before = market.amm.sqrt_k;

    let k_increasing = sqrt_k > market.amm.sqrt_k;

    let new_sqrt_k_u192 = bn::U192::from(sqrt_k);

    let update_k_result = get_update_k_result(market, new_sqrt_k_u192, true)?;

    let adjustment_cost: i128 = math::cp_curve::adjust_k_cost(market, &update_k_result)?;

    math::cp_curve::update_k(market, &update_k_result)?;

    if k_increasing {
        validate!(
            adjustment_cost >= 0,
            ErrorCode::InvalidUpdateK,
            "adjustment_cost negative when k increased"
        )?;
    } else {
        validate!(
            adjustment_cost <= 0,
            ErrorCode::InvalidUpdateK,
            "adjustment_cost positive when k decreased"
        )?;
    }

    if adjustment_cost > 0 {
        let max_cost = market.amm.total_fee_minus_distributions
            .safe_sub(get_total_fee_lower_bound(market)?.cast()?)?
            .safe_sub(market.amm.total_fee_withdrawn.cast()?)?;

        validate!(
            adjustment_cost <= max_cost,
            ErrorCode::InvalidUpdateK,
            "adjustment_cost={} > max_cost={} for k change",
            adjustment_cost,
            max_cost
        )?;
    }

    validate!(
        !k_increasing || market.amm.sqrt_k < MAX_SQRT_K,
        ErrorCode::InvalidUpdateK,
        "cannot increase sqrt_k={} past MAX_SQRT_K",
        market.amm.sqrt_k
    )?;

    validate!(
        market.amm.sqrt_k > market.amm.user_lp_shares,
        ErrorCode::InvalidUpdateK,
        "cannot decrease sqrt_k={} below user_lp_shares={}",
        market.amm.sqrt_k,
        market.amm.user_lp_shares
    )?;

    market.amm.total_fee_minus_distributions =
        market.amm.total_fee_minus_distributions.safe_sub(adjustment_cost)?;

    let amm = &market.amm;

    let price_after = math::amm::calculate_price(
        amm.quote_asset_reserve,
        amm.base_asset_reserve,
        amm.peg_multiplier
    )?;

    let price_change_too_large = price_before
        .cast::<i128>()?
        .safe_sub(price_after.cast::<i128>()?)?
        .unsigned_abs()
        .gt(&MAX_UPDATE_K_PRICE_CHANGE);

    if price_change_too_large {
        msg!("{:?} -> {:?} (> {:?})", price_before, price_after, MAX_UPDATE_K_PRICE_CHANGE);
        return Err(ErrorCode::InvalidUpdateK.into());
    }

    let k_sqrt_check = bn::U192
        ::from(amm.base_asset_reserve)
        .safe_mul(bn::U192::from(amm.quote_asset_reserve))?
        .integer_sqrt()
        .try_to_u128()?;

    let k_err = k_sqrt_check.cast::<i128>()?.safe_sub(amm.sqrt_k.cast::<i128>()?)?;

    if k_err.unsigned_abs() > 100 {
        msg!("k_err={:?}, {:?} != {:?}", k_err, k_sqrt_check, amm.sqrt_k);
        return Err(ErrorCode::InvalidUpdateK.into());
    }

    let peg_multiplier_after = amm.peg_multiplier;
    let base_asset_reserve_after = amm.base_asset_reserve;
    let quote_asset_reserve_after = amm.quote_asset_reserve;
    let sqrt_k_after = amm.sqrt_k;

    msg!("market.amm.peg_multiplier {} -> {}", peg_multiplier_before, peg_multiplier_after);

    msg!(
        "market.amm.base_asset_reserve {} -> {}",
        base_asset_reserve_before,
        base_asset_reserve_after
    );

    msg!(
        "market.amm.quote_asset_reserve {} -> {}",
        quote_asset_reserve_before,
        quote_asset_reserve_after
    );

    msg!("market.amm.sqrt_k {} -> {}", sqrt_k_before, sqrt_k_after);

    let total_fee = amm.total_fee;
    let total_fee_minus_distributions = amm.total_fee_minus_distributions;

    let OraclePriceData { price: oracle_price, .. } = get_oracle_price(
        &market.amm.oracle_source,
        &ctx.accounts.oracle,
        clock.slot
    )?;

    emit!(CurveRecord {
        ts: now,
        record_id: get_then_update_id!(market, next_curve_record_id),
        market_index: market.market_index,
        peg_multiplier_before,
        base_asset_reserve_before,
        quote_asset_reserve_before,
        sqrt_k_before,
        peg_multiplier_after,
        base_asset_reserve_after,
        quote_asset_reserve_after,
        sqrt_k_after,
        base_asset_amount_long,
        base_asset_amount_short,
        base_asset_amount_with_amm,
        number_of_users,
        adjustment_cost,
        total_fee,
        total_fee_minus_distributions,
        oracle_price,
        fill_record: 0,
    });

    Ok(())
}

#[access_control(
    market_valid(&ctx.accounts.market)
    valid_oracle_for_market(&ctx.accounts.oracle, &ctx.accounts.market)
)]
pub fn handle_reset_amm_oracle_twap(ctx: Context<RepegCurve>) -> Result<()> {
    // admin failsafe to reset amm oracle_twap to the mark_twap

    let market = &mut load_mut!(ctx.accounts.market)?;

    msg!("resetting amm oracle twap for perp market {}", market.market_index);
    msg!(
        "market.amm.historical_oracle_data.last_oracle_price_twap: {:?} -> {:?}",
        market.amm.historical_oracle_data.last_oracle_price_twap,
        market.amm.last_mark_price_twap.cast::<i64>()?
    );

    msg!(
        "market.amm.historical_oracle_data.last_oracle_price_twap_ts: {:?} -> {:?}",
        market.amm.historical_oracle_data.last_oracle_price_twap_ts,
        market.amm.last_mark_price_twap_ts
    );

    market.amm.historical_oracle_data.last_oracle_price_twap =
        market.amm.last_mark_price_twap.cast::<i64>()?;
    market.amm.historical_oracle_data.last_oracle_price_twap_ts =
        market.amm.last_mark_price_twap_ts;

    Ok(())
}

// =======

#[access_control(market_valid(&ctx.accounts.market))]
pub fn handle_update_market_name(ctx: Context<AdminUpdateMarket>, name: [u8; 32]) -> Result<()> {
    let mut market = load_mut!(ctx.accounts.market)?;
    msg!("market.name: {:?} -> {:?}", market.name, name);
    market.name = name;
    Ok(())
}

// =======

#[access_control(market_valid(&ctx.accounts.market))]
pub fn handle_update_market_status(
    ctx: Context<AdminUpdateMarket>,
    status: MarketStatus
) -> Result<()> {
    let market = &mut load_mut!(ctx.accounts.market)?;
    msg!("spot market {}", market.market_index);

    msg!("market.status: {:?} -> {:?}", market.status, status);

    market.status = status;
    Ok(())
}

#[access_control(market_valid(&ctx.accounts.market))]
pub fn handle_update_market_paused_operations(
    ctx: Context<AdminUpdateMarket>,
    paused_operations: u8
) -> Result<()> {
    let market = &mut load_mut!(ctx.accounts.market)?;
    msg!("market {}", market.market_index);

    market.paused_operations = paused_operations;

    Operation::log_all_operations_paused(market.paused_operations);

    Ok(())
}

#[access_control(market_valid(&ctx.accounts.market))]
pub fn handle_update_market_synthetic_tier(
    ctx: Context<AdminUpdateMarket>,
    synthetic_tier: SyntheticTier
) -> Result<()> {
    let market = &mut load_mut!(ctx.accounts.market)?;
    msg!("market {}", market.market_index);

    if market.initial_asset_weight > 0 {
        validate!(
            matches!(synthetic_tier, SyntheticTier::Collateral | SyntheticTier::Protected),
            ErrorCode::DefaultError,
            "initial_asset_weight > 0 so SyntheticTier must be collateral or protected"
        )?;
    }

    msg!("market.synthetic_tier: {:?} -> {:?}", market.synthetic_tier, synthetic_tier);

    market.synthetic_tier = synthetic_tier;
    Ok(())
}

#[access_control(market_valid(&ctx.accounts.market))]
pub fn handle_update_market_concentration_coef(
    ctx: Context<AdminUpdateMarket>,
    concentration_scale: u128
) -> Result<()> {
    validate!(concentration_scale > 0, ErrorCode::DefaultError, "invalid concentration_scale")?;

    let market = &mut load_mut!(ctx.accounts.market)?;
    msg!("market {}", market.market_index);

    let prev_concentration_coef = market.amm.concentration_coef;
    controller::amm::update_concentration_coef(market, concentration_scale)?;
    let new_concentration_coef = market.amm.concentration_coef;

    msg!(
        "market.amm.concentration_coef: {} -> {}",
        prev_concentration_coef,
        new_concentration_coef
    );

    Ok(())
}

#[access_control(market_valid(&ctx.accounts.market))]
pub fn handle_update_market_curve_update_intensity(
    ctx: Context<AdminUpdateMarket>,
    curve_update_intensity: u8
) -> Result<()> {
    // (0, 100] is for repeg / formulaic k intensity
    // (100, 200] is for reference price offset intensity
    validate!(
        curve_update_intensity <= 200,
        ErrorCode::DefaultError,
        "invalid curve_update_intensity"
    )?;
    let market = &mut load_mut!(ctx.accounts.market)?;
    msg!("market {}", market.market_index);

    msg!(
        "market.amm.curve_update_intensity: {} -> {}",
        market.amm.curve_update_intensity,
        curve_update_intensity
    );

    market.amm.curve_update_intensity = curve_update_intensity;
    Ok(())
}

#[access_control(market_valid(&ctx.accounts.market))]
pub fn handle_update_market_target_base_asset_amount_per_lp(
    ctx: Context<AdminUpdateMarket>,
    target_base_asset_amount_per_lp: i32
) -> Result<()> {
    let market = &mut load_mut!(ctx.accounts.market)?;
    msg!("market {}", market.market_index);

    msg!(
        "market.amm.target_base_asset_amount_per_lp: {} -> {}",
        market.amm.target_base_asset_amount_per_lp,
        target_base_asset_amount_per_lp
    );

    market.amm.target_base_asset_amount_per_lp = target_base_asset_amount_per_lp;
    Ok(())
}

pub fn handle_update_market_per_lp_base(
    ctx: Context<AdminUpdateMarket>,
    per_lp_base: i8
) -> Result<()> {
    let market = &mut load_mut!(ctx.accounts.market)?;
    msg!("market {}", market.market_index);

    let old_per_lp_base = market.amm.per_lp_base;
    msg!("updated market per_lp_base {} -> {}", old_per_lp_base, per_lp_base);

    let expo_diff = per_lp_base.safe_sub(old_per_lp_base)?;

    validate!(expo_diff.abs() == 1, ErrorCode::DefaultError, "invalid expo update (must be 1)")?;

    validate!(
        per_lp_base.abs() <= 9,
        ErrorCode::DefaultError,
        "only consider lp_base within range of AMM_RESERVE_PRECISION"
    )?;

    controller::lp::apply_lp_rebase_to_market(market, expo_diff)?;

    Ok(())
}

pub fn handle_update_lp_cooldown_time(
    ctx: Context<AdminUpdateState>,
    lp_cooldown_time: u64
) -> Result<()> {
    msg!("lp_cooldown_time: {} -> {}", ctx.accounts.state.lp_cooldown_time, lp_cooldown_time);

    ctx.accounts.state.lp_cooldown_time = lp_cooldown_time;
    Ok(())
}

pub fn handle_update_fee_structure(
    ctx: Context<AdminUpdateState>,
    fee_structure: FeeStructure
) -> Result<()> {
    validate_fee_structure(&fee_structure)?;

    msg!("fee_structure: {:?} -> {:?}", ctx.accounts.state.fee_structure, fee_structure);

    ctx.accounts.state.fee_structure = fee_structure;
    Ok(())
}

pub fn handle_update_oracle_guard_rails(
    ctx: Context<AdminUpdateState>,
    oracle_guard_rails: OracleGuardRails
) -> Result<()> {
    msg!(
        "oracle_guard_rails: {:?} -> {:?}",
        ctx.accounts.state.oracle_guard_rails,
        oracle_guard_rails
    );

    ctx.accounts.state.oracle_guard_rails = oracle_guard_rails;
    Ok(())
}

pub fn handle_update_state_settlement_duration(
    ctx: Context<AdminUpdateState>,
    settlement_duration: u16
) -> Result<()> {
    msg!(
        "settlement_duration: {} -> {}",
        ctx.accounts.state.settlement_duration,
        settlement_duration
    );

    ctx.accounts.state.settlement_duration = settlement_duration;
    Ok(())
}

pub fn handle_update_state_max_number_of_sub_accounts(
    ctx: Context<AdminUpdateState>,
    max_number_of_sub_accounts: u16
) -> Result<()> {
    msg!(
        "max_number_of_sub_accounts: {} -> {}",
        ctx.accounts.state.max_number_of_sub_accounts,
        max_number_of_sub_accounts
    );

    ctx.accounts.state.max_number_of_sub_accounts = max_number_of_sub_accounts;
    Ok(())
}

pub fn handle_update_state_max_initialize_user_fee(
    ctx: Context<AdminUpdateState>,
    max_initialize_user_fee: u16
) -> Result<()> {
    msg!(
        "max_initialize_user_fee: {} -> {}",
        ctx.accounts.state.max_initialize_user_fee,
        max_initialize_user_fee
    );

    ctx.accounts.state.max_initialize_user_fee = max_initialize_user_fee;
    Ok(())
}

#[access_control(market_valid(&ctx.accounts.market))]
pub fn handle_update_market_base_spread(
    ctx: Context<AdminUpdateMarket>,
    base_spread: u32
) -> Result<()> {
    let market = &mut load_mut!(ctx.accounts.market)?;
    msg!("market {}", market.market_index);

    msg!("market.amm.base_spread: {:?} -> {:?}", market.amm.base_spread, base_spread);

    msg!("market.amm.long_spread: {:?} -> {:?}", market.amm.long_spread, base_spread / 2);

    msg!("market.amm.short_spread: {:?} -> {:?}", market.amm.short_spread, base_spread / 2);

    market.amm.base_spread = base_spread;
    market.amm.long_spread = base_spread / 2;
    market.amm.short_spread = base_spread / 2;
    Ok(())
}

#[access_control(market_valid(&ctx.accounts.market))]
pub fn handle_update_amm_jit_intensity(
    ctx: Context<AdminUpdateMarket>,
    amm_jit_intensity: u8
) -> Result<()> {
    validate!(
        (0..=200).contains(&amm_jit_intensity),
        ErrorCode::DefaultError,
        "invalid amm_jit_intensity"
    )?;

    let market = &mut load_mut!(ctx.accounts.market)?;
    msg!("market {}", market.market_index);

    msg!("market.amm.amm_jit_intensity: {} -> {}", market.amm.amm_jit_intensity, amm_jit_intensity);

    market.amm.amm_jit_intensity = amm_jit_intensity;

    Ok(())
}

#[access_control(market_valid(&ctx.accounts.market))]
pub fn handle_update_market_max_spread(
    ctx: Context<AdminUpdateMarket>,
    max_spread: u32
) -> Result<()> {
    let market = &mut load_mut!(ctx.accounts.market)?;
    msg!("market {}", market.market_index);

    validate!(
        max_spread >= market.amm.base_spread,
        ErrorCode::DefaultError,
        "invalid max_spread < base_spread"
    )?;

    msg!("market.amm.max_spread: {:?} -> {:?}", market.amm.max_spread, max_spread);

    market.amm.max_spread = max_spread;

    Ok(())
}

#[access_control(market_valid(&ctx.accounts.market))]
pub fn handle_update_market_step_size_and_tick_size(
    ctx: Context<AdminUpdateMarket>,
    step_size: u64,
    tick_size: u64
) -> Result<()> {
    let market = &mut load_mut!(ctx.accounts.market)?;
    msg!("market {}", market.market_index);

    validate!(step_size > 0 && tick_size > 0, ErrorCode::DefaultError)?;
    validate!(step_size <= 2000000000, ErrorCode::DefaultError)?; // below i32 max for lp's remainder_base_asset

    msg!("market.amm.order_step_size: {:?} -> {:?}", market.amm.order_step_size, step_size);

    msg!("market.amm.order_tick_size: {:?} -> {:?}", market.amm.order_tick_size, tick_size);

    market.amm.order_step_size = step_size;
    market.amm.order_tick_size = tick_size;
    Ok(())
}

#[access_control(market_valid(&ctx.accounts.market))]
pub fn handle_update_market_min_order_size(
    ctx: Context<AdminUpdateMarket>,
    order_size: u64
) -> Result<()> {
    let market = &mut load_mut!(ctx.accounts.market)?;
    msg!("market {}", market.market_index);

    validate!(order_size > 0, ErrorCode::DefaultError)?;

    msg!("market.amm.min_order_size: {:?} -> {:?}", market.amm.min_order_size, order_size);

    market.amm.min_order_size = order_size;
    Ok(())
}

#[access_control(market_valid(&ctx.accounts.market))]
pub fn handle_update_market_max_slippage_ratio(
    ctx: Context<AdminUpdateMarket>,
    max_slippage_ratio: u16
) -> Result<()> {
    validate!(max_slippage_ratio > 0, ErrorCode::DefaultError)?;
    let market = &mut load_mut!(ctx.accounts.market)?;
    msg!("market {}", market.market_index);

    msg!(
        "market.amm.max_slippage_ratio: {:?} -> {:?}",
        market.amm.max_slippage_ratio,
        max_slippage_ratio
    );

    market.amm.max_slippage_ratio = max_slippage_ratio;
    Ok(())
}

#[access_control(market_valid(&ctx.accounts.market))]
pub fn handle_update_market_max_fill_reserve_fraction(
    ctx: Context<AdminUpdateMarket>,
    max_fill_reserve_fraction: u16
) -> Result<()> {
    validate!(max_fill_reserve_fraction > 0, ErrorCode::DefaultError)?;
    let market = &mut load_mut!(ctx.accounts.market)?;
    msg!("market {}", market.market_index);

    msg!(
        "market.amm.max_fill_reserve_fraction: {:?} -> {:?}",
        market.amm.max_fill_reserve_fraction,
        max_fill_reserve_fraction
    );

    market.amm.max_fill_reserve_fraction = max_fill_reserve_fraction;
    Ok(())
}

#[access_control(market_valid(&ctx.accounts.market))]
pub fn handle_update_market_max_open_interest(
    ctx: Context<AdminUpdateMarket>,
    max_open_interest: u128
) -> Result<()> {
    let market = &mut load_mut!(ctx.accounts.market)?;
    msg!("market {}", market.market_index);

    validate!(
        is_multiple_of_step_size(max_open_interest.cast::<u64>()?, market.amm.order_step_size)?,
        ErrorCode::DefaultError,
        "max oi not a multiple of the step size"
    )?;

    msg!(
        "market.amm.max_open_interest: {:?} -> {:?}",
        market.amm.max_open_interest,
        max_open_interest
    );

    market.amm.max_open_interest = max_open_interest;
    Ok(())
}

#[access_control(market_valid(&ctx.accounts.market))]
pub fn handle_update_market_fee_adjustment(
    ctx: Context<AdminUpdateMarket>,
    fee_adjustment: i16
) -> Result<()> {
    let market = &mut load_mut!(ctx.accounts.market)?;
    msg!("market {}", market.market_index);

    validate!(
        fee_adjustment.unsigned_abs().cast::<u64>()? <= FEE_ADJUSTMENT_MAX,
        ErrorCode::DefaultError,
        "fee adjustment {} greater than max {}",
        fee_adjustment,
        FEE_ADJUSTMENT_MAX
    )?;

    msg!("market.fee_adjustment: {:?} -> {:?}", market.fee_adjustment, fee_adjustment);

    market.fee_adjustment = fee_adjustment;
    Ok(())
}

pub fn handle_update_market_number_of_users(
    ctx: Context<AdminUpdateMarket>,
    number_of_users: Option<u32>,
    number_of_users_with_base: Option<u32>
) -> Result<()> {
    let market = &mut load_mut!(ctx.accounts.market)?;
    msg!("market {}", market.market_index);

    if let Some(number_of_users) = number_of_users {
        msg!("market.number_of_users: {:?} -> {:?}", market.number_of_users, number_of_users);
        market.number_of_users = number_of_users;
    } else {
        msg!("market.number_of_users: unchanged");
    }

    if let Some(number_of_users_with_base) = number_of_users_with_base {
        msg!(
            "market.number_of_users_with_base: {:?} -> {:?}",
            market.number_of_users_with_base,
            number_of_users_with_base
        );
        market.number_of_users_with_base = number_of_users_with_base;
    } else {
        msg!("market.number_of_users_with_base: unchanged");
    }

    validate!(
        market.number_of_users >= market.number_of_users_with_base,
        ErrorCode::DefaultError,
        "number_of_users must be >= number_of_users_with_base "
    )?;

    Ok(())
}


pub fn handle_initialize_insurance_fund(
    ctx: Context<InitializeInsuranceFund>,
    insurance_fund_total_factor: u32
) -> Result<()> {
    let state = &mut ctx.accounts.state;
    let insurance_fund_pubkey = ctx.accounts.insurance_fund.key();

    // protocol must be authority of collateral vault
    if ctx.accounts.market_vault.owner != state.signer {
        return Err(ErrorCode::InvalidMarketAuthority.into());
    }

    let market_index = get_then_update_id!(state, number_of_markets);

    msg!("initializing insurance fund");

    let market = &mut ctx.accounts.market.load_init()?;
    let clock = Clock::get()?;
    let now = clock.unix_timestamp.cast().or(Err(ErrorCode::UnableToCastUnixTime))?;

    let decimals = ctx.accounts.market_mint.decimals.cast::<u32>()?;

    **insurance_fund = InsuranceFund {
        // pubkey: insurance_fund_pubkey,
        vault: *ctx.accounts.insurance_fund_vault.to_account_info().key,
        unstaking_period: THIRTEEN_DAY,
        total_factor: insurance_fund_total_factor,
        user_factor: insurance_fund_total_factor / 2,
        ..InsuranceFund::default()
        },
    };

    Ok(())
}


#[access_control(
    spot_market_valid(&ctx.accounts.spot_market)
)]
pub fn handle_update_insurance_fund_factor(
    ctx: Context<AdminUpdateInsuranceFund>,
    user_insurance_fund_factor: u32,
    total_insurance_fund_factor: u32,
) -> Result<()> {
    let insurance_fund = &mut load_mut!(ctx.accounts.insurance_fund)?;

    validate!(
        user_insurance_fund_factor <= total_insurance_fund_factor,
        ErrorCode::DefaultError,
        "user_insurance_fund_factor must be <= total_insurance_fund_factor"
    )?;

    validate!(
        total_insurance_fund_factor <= IF_FACTOR_PRECISION.cast()?,
        ErrorCode::DefaultError,
        "total_insurance_fund_factor must be <= 100%"
    )?;

    msg!(
        "user_insurance_fund_factor: {:?} -> {:?}",
        insurance_fund.user_factor,
        user_insurance_fund_factor
    );
    msg!(
        "total_insurance_fund_factor: {:?} -> {:?}",
        insurance_fund.total_factor,
        total_insurance_fund_factor
    );

    insurance_fund.user_factor = user_insurance_fund_factor;
    insurance_fund.total_factor = total_insurance_fund_factor;

    Ok(())
}

pub fn handle_update_insurance_fund_paused_operations(
    ctx: Context<AdminUpdateInsuranceFund>,
    paused_operations: u8,
) -> Result<()> {
    let insurance_fund = &mut load_mut!(ctx.accounts.insurance_fund)?;
    insurance_fund.paused_operations = paused_operations;
    InsuranceFundOperation::log_all_operations_paused(paused_operations);
    Ok(())
}


pub fn handle_initialize_protocol_insurance_fund_shares_transfer_config(
    ctx: Context<InitializeProtocolInsuranceFundSharesTransferConfig>,
) -> Result<()> {
    let mut config = ctx
        .accounts
        .protocol_insurance_fund_shares_transfer_config
        .load_init()?;

    let now = Clock::get()?.unix_timestamp;
    msg!(
        "next_epoch_ts: {:?} -> {:?}",
        config.next_epoch_ts,
        now.safe_add(EPOCH_DURATION)?
    );
    config.next_epoch_ts = now.safe_add(EPOCH_DURATION)?;

    Ok(())
}

pub fn handle_update_protocol_insurance_fund_shares_transfer_config(
    ctx: Context<UpdateProtocolInsuranceFundSharesTransferConfig>,
    whitelisted_signers: Option<[Pubkey; 4]>,
    max_transfer_per_epoch: Option<u128>,
) -> Result<()> {
    let mut config = ctx.accounts.protocol_insurance_fund_shares_transfer_config.load_mut()?;

    if let Some(whitelisted_signers) = whitelisted_signers {
        msg!(
            "whitelisted_signers: {:?} -> {:?}",
            config.whitelisted_signers,
            whitelisted_signers
        );
        config.whitelisted_signers = whitelisted_signers;
    } else {
        msg!("whitelisted_signers: unchanged");
    }

    if let Some(max_transfer_per_epoch) = max_transfer_per_epoch {
        msg!(
            "max_transfer_per_epoch: {:?} -> {:?}",
            config.max_transfer_per_epoch,
            max_transfer_per_epoch
        );
        config.max_transfer_per_epoch = max_transfer_per_epoch;
    } else {
        msg!("max_transfer_per_epoch: unchanged");
    }

    Ok(())
}



pub fn handle_tranfer_fees_to_insurance_fund<'c: 'info, 'info>(
    ctx: Context<'_, '_, 'c, 'info, TransferFeesToInsuranceFund<'info>>,
    market_index: u16,
) -> Result<()> {
    let state = &ctx.accounts.state;
    let market = &mut load_mut!(ctx.accounts.market)?;
    let insurance_fund = &mut load_mut!(ctx.accounts.insurance_fund)?;

    let remaining_accounts_iter = &mut ctx.remaining_accounts.iter().peekable();
    let mint = get_token_mint(remaining_accounts_iter)?;

    validate!(
       market_index == market.market_index,
        ErrorCode::InvalidMarketAccount,
        "invalid market passed"
    )?;

    // TODO: ensure insurance fund max limit not reached
    // ...

    let market_vault_amount = ctx.accounts.market_vault.amount;
    let insurance_vault_amount = ctx.accounts.insurance_fund_vault.amount;

    let clock = Clock::get()?;
    let now = clock.unix_timestamp;

    // uses proportion of revenue pool allocated to insurance fund
    let token_amount = controller::insurance::transfer_fees_to_insurance_fund(
        market_vault_amount,
        insurance_vault_amount,
        market,
        insurance_fund,
        now,
        true,
    )?;

    insurance_fund.last_fee_deposit_ts = now;

    controller::token::send_from_program_vault(
        &ctx.accounts.token_program,
        &ctx.accounts.market_fee_pool,
        &ctx.accounts.insurance_fund_vault,
        &ctx.accounts.normal_signer,
        state.signer_nonce,
        token_amount,
        &mint,
    )?;

    // reload the market vault balance so it's up-to-date
    ctx.accounts.market_vault.reload()?;

    Ok(())
}

pub fn handle_transfer_fees_to_treasury<'c: 'info, 'info>(
    ctx: Context<'_, '_, 'c, 'info, TransferFeesToTreasury<'info>>,
    market_index: u16,
) -> Result<()> {
    let state = &ctx.accounts.state;
    let market = &mut load_mut!(ctx.accounts.market)?;
   

    let remaining_accounts_iter = &mut ctx.remaining_accounts.iter().peekable();
    let mint = get_token_mint(remaining_accounts_iter)?;

    validate!(
       market_index == market.market_index,
        ErrorCode::InvalidMarketAccount,
        "invalid market passed"
    )?;

    let market_vault_amount = ctx.accounts.market_vault.amount;
    let insurance_vault_amount = ctx.accounts.insurance_fund_vault.amount;
    let insurance_fund = &mut load_mut!(ctx.accounts.insurance_fund)?;

    let clock = Clock::get()?;
    let now = clock.unix_timestamp;

    // uses proportion of revenue pool allocated to insurance fund
    let token_amount = controller::insurance::transfer_fees_to_treasury(
        market_vault_amount,
        insurance_vault_amount,
        market,
        insurance_fund,
        now,
        true,
    )?;

    controller::token::send_from_program_vault(
        &ctx.accounts.token_program,
        &ctx.accounts.market_fee_pool,
        &ctx.accounts.treasury_vault,
        &ctx.accounts.normal_signer,
        state.signer_nonce,
        token_amount,
        &mint,
    )?;

    // reload the market vault balance so it's up-to-date
    ctx.accounts.market_vault.reload()?;

    Ok(())
}



pub fn handle_burn_gov_token_with_fees<'c: 'info, 'info>(
    ctx: Context<'_, '_, 'c, 'info, TransferFeesToTreasury<'info>>,
    market_index: u16,
) -> Result<()> {
    // 1) Purchase NORM with fee amount

    // 2) Burn NORM - TODO: this is not complete
    controller::token::burn_tokens(
        &ctx.accounts.governance_token_program,
        &ctx.accounts.market_fee_pool,
        &ctx.accounts.market_vault,
        state.signer_nonce,
        token_amount,
        &mint,
    );


    Ok(())
}


pub fn handle_update_admin(ctx: Context<AdminUpdateState>, admin: Pubkey) -> Result<()> {
    msg!("admin: {:?} -> {:?}", ctx.accounts.state.admin, admin);
    ctx.accounts.state.admin = admin;
    Ok(())
}

pub fn handle_update_whitelist_mint(
    ctx: Context<AdminUpdateState>,
    whitelist_mint: Pubkey
) -> Result<()> {
    msg!("whitelist_mint: {:?} -> {:?}", ctx.accounts.state.whitelist_mint, whitelist_mint);

    ctx.accounts.state.whitelist_mint = whitelist_mint;
    Ok(())
}

pub fn handle_update_discount_mint(
    ctx: Context<AdminUpdateState>,
    discount_mint: Pubkey
) -> Result<()> {
    msg!("discount_mint: {:?} -> {:?}", ctx.accounts.state.discount_mint, discount_mint);

    ctx.accounts.state.discount_mint = discount_mint;
    Ok(())
}

pub fn handle_update_exchange_status(
    ctx: Context<AdminUpdateState>,
    exchange_status: u8
) -> Result<()> {
    msg!("exchange_status: {:?} -> {:?}", ctx.accounts.state.exchange_status, exchange_status);

    ctx.accounts.state.exchange_status = exchange_status;
    Ok(())
}

pub fn handle_update_auction_duration(
    ctx: Context<AdminUpdateState>,
    min_auction_duration: u8
) -> Result<()> {
    msg!(
        "min_auction_duration: {:?} -> {:?}",
        ctx.accounts.state.min_auction_duration,
        min_auction_duration
    );

    ctx.accounts.state.min_auction_duration = min_auction_duration;
    Ok(())
}

pub fn handle_admin_disable_update_bid_ask_twap(
    ctx: Context<AdminDisableBidAskTwapUpdate>,
    disable: bool
) -> Result<()> {
    let mut user_stats = load_mut!(ctx.accounts.user_stats)?;

    msg!(
        "disable_update_bid_ask_twap: {:?} -> {:?}",
        user_stats.disable_update_bid_ask_twap,
        disable
    );

    user_stats.disable_update_bid_ask_twap = disable;
    Ok(())
}

pub fn handle_initialize_pyth_pull_oracle(
    ctx: Context<InitPythPullPriceFeed>,
    feed_id: [u8; 32]
) -> Result<()> {
    let cpi_program = ctx.accounts.pyth_solana_receiver.to_account_info();
    let cpi_accounts = InitPriceUpdate {
        payer: ctx.accounts.admin.to_account_info(),
        price_update_account: ctx.accounts.price_feed.to_account_info(),
        system_program: ctx.accounts.system_program.to_account_info(),
        write_authority: ctx.accounts.price_feed.to_account_info(),
    };

    let seeds = &[PTYH_PRICE_FEED_SEED_PREFIX, feed_id.as_ref(), &[ctx.bumps.price_feed]];
    let signer_seeds = &[&seeds[..]];
    let cpi_context = CpiContext::new_with_signer(cpi_program, cpi_accounts, signer_seeds);

    pyth_solana_receiver_sdk::cpi::init_price_update(cpi_context, feed_id)?;

    Ok(())
}

pub fn handle_settle_expired_market<'c: 'info, 'info>(
    ctx: Context<'_, '_, 'c, 'info, AdminUpdateMarket<'info>>,
    market_index: u16
) -> Result<()> {
    let clock = Clock::get()?;
    let _now = clock.unix_timestamp;
    let state = &ctx.accounts.state;

    let AccountMaps { market_map, mut oracle_map } = load_maps(
        &mut ctx.remaining_accounts.iter().peekable(),
        &get_writable_market_set(market_index),
        // &get_writable_spot_market_set(QUOTE_SPOT_MARKET_INDEX),
        clock.slot,
        Some(state.oracle_guard_rails)
    )?;

    controller::repeg::update_amm(market_index, &market_map, &mut oracle_map, state, &clock)?;

    controller::repeg::settle_expired_market(
        market_index,
        &market_map,
        &mut oracle_map,
        state,
        &clock
    )?;

    Ok(())
}

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,
    #[account(init, seeds = [b"normal_state".as_ref()], space = State::SIZE, bump, payer = admin)]
    pub state: Box<Account<'info, State>>,
    pub quote_asset_mint: Box<InterfaceAccount<'info, Mint>>,
    /// CHECK: checked in `initialize`
    pub normal_signer: AccountInfo<'info>,
    pub rent: Sysvar<'info, Rent>,
    pub system_program: Program<'info, System>,
    pub token_program: Interface<'info, TokenInterface>,
}

#[access_control(has_been_approved(&ctx.accounts.market))]
#[derive(Accounts)]
pub struct InitializeMarket<'info> {
    #[account(
        init,
        seeds = [b"market", state.number_of_markets.to_le_bytes().as_ref()],
        space = Market::SIZE,
        bump,
        payer = admin
    )]
    pub market: AccountLoader<'info, Market>,
    pub market_mint: Box<InterfaceAccount<'info, Mint>>,
    #[account(
        init,
        seeds = [b"market_vault".as_ref(), state.number_of_markets.to_le_bytes().as_ref()],
        bump,
        payer = admin,
        token::mint = market_mint,
        token::authority = normal_signer
    )]
    pub market_vault: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(constraint = state.signer.eq(&normal_signer.key()))]
    /// CHECK: program signer
    pub normal_signer: AccountInfo<'info>,
    #[account(
        mut,
        has_one = admin
    )]
    pub state: Box<Account<'info, State>>,
    /// CHECK: checked in `initialize_market`
    pub oracle: AccountInfo<'info>,
    pub insurance_fund: AccountInfo<'info>,
    #[account(mut)]
    pub admin: Signer<'info>,
    pub rent: Sysvar<'info, Rent>,
    pub system_program: Program<'info, System>,
    pub token_program: Interface<'info, TokenInterface>,
    // TODO: additions...
    // The governance program account (spl-governance)
    #[account(executable)]
    pub governance_program: Program<'info, Governance>,
}

#[derive(Accounts)]
#[instruction(market_index: u16)]
pub struct DeleteInitializedMarket<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,
    #[account(
        mut,
        has_one = admin
    )]
    pub state: Box<Account<'info, State>>,
    #[account(mut, close = admin)]
    pub market: AccountLoader<'info, SpotMarket>,
    #[account(
        mut,
        seeds = [b"market_vault".as_ref(), market_index.to_le_bytes().as_ref()],
        bump,
    )]
    pub market_vault: Box<InterfaceAccount<'info, TokenAccount>>,
    /// CHECK: program signer
    pub normal_signer: AccountInfo<'info>,
    pub token_program: Interface<'info, TokenInterface>,
}

#[derive(Accounts)]
pub struct AdminUpdateMarketAmmSummaryStats<'info> {
    #[account(address = admin_hot_wallet::id())]
    pub admin: Signer<'info>,
    pub state: Box<Account<'info, State>>,
    #[account(mut)]
    pub market: AccountLoader<'info, Market>,
    /// CHECK: checked in `admin_update_perp_market_summary_stats` ix constraint
    pub oracle: AccountInfo<'info>,
}

#[derive(Accounts)]
pub struct SettleExpiredMarketPoolsToRevenuePool<'info> {
    #[account(has_one = admin)]
    pub state: Box<Account<'info, State>>,
    pub admin: Signer<'info>,
    #[account(
        seeds = [b"market", 0_u16.to_le_bytes().as_ref()],
        bump,
        mut
    )]
    pub market: AccountLoader<'info, SpotMarket>,
    #[account(mut)]
    pub market: AccountLoader<'info, Market>,
}

#[derive(Accounts)]
pub struct RepegCurve<'info> {
    #[account(has_one = admin)]
    pub state: Box<Account<'info, State>>,
    #[account(mut)]
    pub market: AccountLoader<'info, Market>,
    /// CHECK: checked in `repeg_curve` ix constraint
    pub oracle: AccountInfo<'info>,
    pub admin: Signer<'info>,
}

#[derive(Accounts)]
pub struct AdminUpdateState<'info> {
    pub admin: Signer<'info>,
    #[account(
        mut,
        has_one = admin
    )]
    pub state: Box<Account<'info, State>>,
}

#[derive(Accounts)]
pub struct AdminUpdateK<'info> {
    pub admin: Signer<'info>,
    #[account(has_one = admin)]
    pub state: Box<Account<'info, State>>,
    #[account(mut)]
    pub perp_market: AccountLoader<'info, PerpMarket>,
    /// CHECK: checked in `admin_update_k` ix constraint
    pub oracle: AccountInfo<'info>,
}

#[derive(Accounts)]
pub struct AdminUpdateMarket<'info> {
    pub admin: Signer<'info>,
    #[account(has_one = admin)]
    pub state: Box<Account<'info, State>>,
    #[account(mut)]
    pub market: AccountLoader<'info, Market>,
}

#[derive(Accounts)]
pub struct AdminUpdateMarketOracle<'info> {
    pub admin: Signer<'info>,
    #[account(has_one = admin)]
    pub state: Box<Account<'info, State>>,
    #[account(mut)]
    pub market: AccountLoader<'info, Market>,
    /// CHECK: checked in `initialize_spot_market`
    pub oracle: AccountInfo<'info>,
}

#[derive(Accounts)]
pub struct AdminDisableBidAskTwapUpdate<'info> {
    pub admin: Signer<'info>,
    #[account(has_one = admin)]
    pub state: Box<Account<'info, State>>,
    #[account(mut)]
    pub user_stats: AccountLoader<'info, UserStats>,
}

#[derive(Accounts)]
#[instruction(feed_id : [u8; 32])]
pub struct InitPythPullPriceFeed<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,
    pub pyth_solana_receiver: Program<'info, PythSolanaReceiver>,
    /// CHECK: This account's seeds are checked
    #[account(mut, seeds = [PTYH_PRICE_FEED_SEED_PREFIX, &feed_id], bump)]
    pub price_feed: AccountInfo<'info>,
    pub system_program: Program<'info, System>,
    #[account(has_one = admin)]
    pub state: Box<Account<'info, State>>,
}

/// Insurance Fund

// TODO: finish
#[derive(Accounts)]
pub struct InitializeInsuranceFund<'info> {
    #[account(
        init,
        seeds = [b"market", state.number_of_markets.to_le_bytes().as_ref()],
        space = Market::SIZE,
        bump,
        payer = admin
    )]
    pub market: AccountLoader<'info, Market>,
    pub market_mint: Box<InterfaceAccount<'info, Mint>>,
    #[account(
        init,
        seeds = [b"market_vault".as_ref(), state.number_of_markets.to_le_bytes().as_ref()],
        bump,
        payer = admin,
        token::mint = market_mint,
        token::authority = normal_signer
    )]
    pub market_vault: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(constraint = state.signer.eq(&normal_signer.key()))]
    /// CHECK: program signer
    pub normal_signer: AccountInfo<'info>,
    #[account(
        mut,
        has_one = admin
    )]
    pub state: Box<Account<'info, State>>,
    /// CHECK: checked in `initialize_market`
   
    #[account(mut)]
    pub admin: Signer<'info>,
    pub rent: Sysvar<'info, Rent>,
    pub system_program: Program<'info, System>,
    pub token_program: Interface<'info, TokenInterface>
}

#[derive(Accounts)]
pub struct AdminUpdateInsuranceFund<'info> {
    pub admin: Signer<'info>,
    #[account(has_one = admin)]
    pub state: Box<Account<'info, State>>,
    #[account(mut)]
    pub insurance_fund: AccountLoader<'info, InsuranceFund>,
}


#[derive(Accounts)]
pub struct InitializeProtocolInsuranceFundSharesTransferConfig<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,
    #[account(
        init,
        seeds = [b"insurance_fund_shares_transfer_config".as_ref()],
        space = ProtocolInsuranceFundSharesTransferConfig::SIZE,
        bump,
        payer = admin
    )]
    pub protocol_insurance_fund_shares_transfer_config: AccountLoader<'info, ProtocolInsuranceFundSharesTransferConfig>,
    #[account(
        has_one = admin
    )]
    pub state: Box<Account<'info, State>>,
    pub rent: Sysvar<'info, Rent>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct UpdateProtocolInsuranceFundSharesTransferConfig<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,
    #[account(
        mut,
        seeds = [b"insurance_fund_shares_transfer_config".as_ref()],
        bump,
    )]
    pub protocol_insurance_fund_shares_transfer_config: AccountLoader<'info, ProtocolInsuranceFundSharesTransferConfig>,
    #[account(
        has_one = admin
    )]
    pub state: Box<Account<'info, State>>,
}

#[derive(Accounts)]
#[instruction(market_index: u16,)]
pub struct TransferFeesToInsuranceFund<'info> {
	pub state: Box<Account<'info, State>>,
	#[account(
        mut,
        seeds = [b"market", market_index.to_le_bytes().as_ref()],
        bump
    )]
	pub market: AccountLoader<'info, Market>,
	#[account(
        mut,
        seeds = [b"market_fee_pool".as_ref(), market_index.to_le_bytes().as_ref()],
        bump,
    )]
	pub market_fee_pool: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(
        mut,
        seeds = [b"insurance_fund"],
        bump
    )]
	pub insurance_fund: AccountLoader<'info, InsuranceFund>,
	#[account(constraint = state.signer.eq(&normal_signer.key()))]
	/// CHECK: forced normal_signer
	pub normal_signer: AccountInfo<'info>,
	#[account(
        mut,
        seeds = [b"insurance_fund_vault".as_ref()],
        bump,
    )]
	pub insurance_fund_vault: Box<InterfaceAccount<'info, TokenAccount>>,
	pub token_program: Interface<'info, TokenInterface>,
}



#[derive(Accounts)]
#[instruction(market_index: u16,)]
pub struct TransferFeesToTreasury<'info> {
	pub state: Box<Account<'info, State>>,
	#[account(
        mut,
        seeds = [b"market", market_index.to_le_bytes().as_ref()],
        bump
    )]
	pub market: AccountLoader<'info, Market>,
	#[account(
        mut,
        seeds = [b"market_fee_pool".as_ref(), market_index.to_le_bytes().as_ref()],
        bump,
    )]
	pub market_fee_pool: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(
        mut,
        seeds = [b"insurance_fund"],
        bump
    )]
	pub insurance_fund: AccountLoader<'info, InsuranceFund>,
	#[account(constraint = state.signer.eq(&normal_signer.key()))]
	/// CHECK: forced normal_signer
	pub normal_signer: AccountInfo<'info>,
	#[account(
        mut,
        seeds = [b"treasury_vault".as_ref()],
        bump,
    )]
	pub treasury_vault: Box<InterfaceAccount<'info, TokenAccount>>,
	pub token_program: Interface<'info, TokenInterface>,
}

#[derive(Accounts)]
#[instruction(market_index: u16,)]
pub struct BurnGovTokenWithFees<'info> {
	pub state: Box<Account<'info, State>>,
	#[account(
        mut,
        seeds = [b"market", market_index.to_le_bytes().as_ref()],
        bump
    )]
	pub market: AccountLoader<'info, Market>,
	#[account(
        mut,
        seeds = [b"market_fee_pool".as_ref(), market_index.to_le_bytes().as_ref()],
        bump,
    )]
	pub market_fee_pool: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(
        mut,
        seeds = [b"insurance_fund"],
        bump
    )]
	pub insurance_fund: AccountLoader<'info, InsuranceFund>,
	#[account(constraint = state.signer.eq(&normal_signer.key()))]
	/// CHECK: forced normal_signer
	pub normal_signer: AccountInfo<'info>,
	#[account(
        mut,
        seeds = [b"treasury_vault".as_ref()],
        bump,
    )]
	pub treasury_vault: Box<InterfaceAccount<'info, TokenAccount>>,
	pub token_program: Interface<'info, TokenInterface>,
}