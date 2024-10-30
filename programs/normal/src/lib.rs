#![allow(clippy::too_many_arguments)]
#![allow(clippy::bool_assert_comparison)]
#![allow(clippy::comparison_chain)]

use anchor_lang::prelude::*;

use instructions::*;
#[cfg(test)]
use math::amm;
use math::{ bn, constants::* };
use state::oracle::OracleSource;

use crate::controller::position::OrderSide;
use crate::state::order_params::{ ModifyOrderParams, OrderParams };
use crate::state::market::{ SyntheticTier, MarketStatus };
use crate::state::state::FeeStructure;
use crate::state::state::*;
use crate::state::user::MarketType;

pub mod controller;
pub mod error;
pub mod ids;
pub mod instructions;
pub mod macros;
pub mod math;
mod signer;
pub mod state;
#[cfg(test)]
mod test_utils;
mod validation;

#[cfg(feature = "mainnet-beta")]
declare_id!("dRiftyHA39MWEi3m9aunc5MzRF1JYuBsbn6VPcn33UH");
#[cfg(not(feature = "mainnet-beta"))]
declare_id!("dRiftyHA39MWEi3m9aunc5MzRF1JYuBsbn6VPcn33UH");

#[program]
pub mod normal {
	use super::*;

	/**
	 *
	 * USER INSTRUCTIONS
	 *
	 */

	pub fn initialize_user<'c: 'info, 'info>(
		ctx: Context<'_, '_, 'c, 'info, InitializeUser<'info>>,
		sub_account_id: u16,
		name: [u8; 32]
	) -> Result<()> {
		handle_initialize_user(ctx, sub_account_id, name)
	}

	pub fn initialize_user_stats<'c: 'info, 'info>(
		ctx: Context<'_, '_, 'c, 'info, InitializeUserStats>
	) -> Result<()> {
		handle_initialize_user_stats(ctx)
	}

	pub fn initialize_referrer_name(
		ctx: Context<InitializeReferrerName>,
		name: [u8; 32]
	) -> Result<()> {
		handle_initialize_referrer_name(ctx, name)
	}

	pub fn update_user_name(
		ctx: Context<UpdateUser>,
		_sub_account_id: u16,
		name: [u8; 32]
	) -> Result<()> {
		handle_update_user_name(ctx, _sub_account_id, name)
	}

	pub fn update_user_delegate(
		ctx: Context<UpdateUser>,
		_sub_account_id: u16,
		delegate: Pubkey
	) -> Result<()> {
		handle_update_user_delegate(ctx, _sub_account_id, delegate)
	}

	pub fn update_user_reduce_only(
		ctx: Context<UpdateUser>,
		_sub_account_id: u16,
		reduce_only: bool
	) -> Result<()> {
		handle_update_user_reduce_only(ctx, _sub_account_id, reduce_only)
	}

	pub fn delete_user<'c: 'info, 'info>(
		ctx: Context<'_, '_, 'c, 'info, DeleteUser>
	) -> Result<()> {
		handle_delete_user(ctx)
	}

	pub fn reclaim_rent(ctx: Context<ReclaimRent>) -> Result<()> {
		handle_reclaim_rent(ctx)
	}

	/**
	 *
	 * ORDER INSTRUCTIONS
	 *
	 */

	pub fn place_order<'c: 'info, 'info>(
		ctx: Context<'_, '_, 'c, 'info, PlaceOrder>,
		params: OrderParams
	) -> Result<()> {
		handle_place_order(ctx, params)
	}

	pub fn place_swift_taker_order<'c: 'info, 'info>(
		ctx: Context<'_, '_, 'c, 'info, PlaceSwiftTakerOrder<'info>>,
		swift_message_bytes: Vec<u8>,
		swift_order_params_message_bytes: Vec<u8>,
		swift_message_signature: [u8; 64]
	) -> Result<()> {
		handle_place_swift_taker_order(
			ctx,
			swift_message_bytes,
			swift_order_params_message_bytes,
			swift_message_signature
		)
	}

	pub fn place_and_take_order<'c: 'info, 'info>(
		ctx: Context<'_, '_, 'c, 'info, PlaceAndTake<'info>>,
		params: OrderParams,
		fulfillment_type: Option<FulfillmentType>,
		maker_order_id: Option<u32>
	) -> Result<()> {
		handle_place_and_take_order(ctx, params, maker_order_id)
	}

	pub fn place_and_make_order<'c: 'info, 'info>(
		ctx: Context<'_, '_, 'c, 'info, PlaceAndMake<'info>>,
		params: OrderParams,
		taker_order_id: u32,
		fulfillment_type: Option<FulfillmentType>
	) -> Result<()> {
		handle_place_and_make_order(ctx, params, taker_order_id)
	}

	pub fn place_orders<'c: 'info, 'info>(
		ctx: Context<'_, '_, 'c, 'info, PlaceOrder>,
		params: Vec<OrderParams>
	) -> Result<()> {
		handle_place_orders(ctx, params)
	}

	pub fn cancel_order<'c: 'info, 'info>(
		ctx: Context<'_, '_, 'c, 'info, CancelOrder>,
		order_id: Option<u32>
	) -> Result<()> {
		handle_cancel_order(ctx, order_id)
	}

	pub fn cancel_order_by_user_id<'c: 'info, 'info>(
		ctx: Context<'_, '_, 'c, 'info, CancelOrder>,
		user_order_id: u8
	) -> Result<()> {
		handle_cancel_order_by_user_id(ctx, user_order_id)
	}

	pub fn cancel_orders<'c: 'info, 'info>(
		ctx: Context<'_, '_, 'c, 'info, CancelOrder<'info>>,
		market_type: Option<MarketType>,
		market_index: Option<u16>,
		side: Option<OrderSide>
	) -> Result<()> {
		handle_cancel_orders(ctx, market_type, market_index, side)
	}

	pub fn cancel_orders_by_ids<'c: 'info, 'info>(
		ctx: Context<'_, '_, 'c, 'info, CancelOrder>,
		order_ids: Vec<u32>
	) -> Result<()> {
		handle_cancel_orders_by_ids(ctx, order_ids)
	}

	pub fn modify_order<'c: 'info, 'info>(
		ctx: Context<'_, '_, 'c, 'info, CancelOrder<'info>>,
		order_id: Option<u32>,
		modify_order_params: ModifyOrderParams
	) -> Result<()> {
		handle_modify_order(ctx, order_id, modify_order_params)
	}

	pub fn modify_order_by_user_id<'c: 'info, 'info>(
		ctx: Context<'_, '_, 'c, 'info, CancelOrder<'info>>,
		user_order_id: u8,
		modify_order_params: ModifyOrderParams
	) -> Result<()> {
		handle_modify_order_by_user_order_id(
			ctx,
			user_order_id,
			modify_order_params
		)
	}

	/**
	 *
	 * KEEPER INSTRUCTIONS
	 *
	 */

	pub fn revert_fill(ctx: Context<RevertFill>) -> Result<()> {
		handle_revert_fill(ctx)
	}

	pub fn fill_order<'c: 'info, 'info>(
		ctx: Context<'_, '_, 'c, 'info, FillOrder<'info>>,
		order_id: Option<u32>
	) -> Result<()> {
		handle_fill_order(ctx, order_id)
	}

	pub fn trigger_order<'c: 'info, 'info>(
		ctx: Context<'_, '_, 'c, 'info, TriggerOrder<'info>>,
		order_id: u32
	) -> Result<()> {
		handle_trigger_order(ctx, order_id)
	}

	pub fn force_cancel_orders<'c: 'info, 'info>(
		ctx: Context<'_, '_, 'c, 'info, ForceCancelOrder<'info>>
	) -> Result<()> {
		handle_force_cancel_orders(ctx)
	}

	pub fn update_user_idle<'c: 'info, 'info>(
		ctx: Context<'_, '_, 'c, 'info, UpdateUserIdle<'info>>
	) -> Result<()> {
		handle_update_user_idle(ctx)
	}

	pub fn update_user_open_orders_count(
		ctx: Context<UpdateUserIdle>
	) -> Result<()> {
		handle_update_user_open_orders_count(ctx)
	}

	pub fn admin_disable_update_bid_ask_twap(
		ctx: Context<AdminDisableBidAskTwapUpdate>,
		disable: bool
	) -> Result<()> {
		handle_admin_disable_update_bid_ask_twap(ctx, disable)
	}

	pub fn settle_expired_market<'c: 'info, 'info>(
		ctx: Context<'_, '_, 'c, 'info, AdminUpdateMarket<'info>>,
		market_index: u16
	) -> Result<()> {
		handle_settle_expired_market(ctx, market_index)
	}

	pub fn update_bid_ask_twap<'c: 'info, 'info>(
		ctx: Context<'_, '_, 'c, 'info, UpdateBidAskTwap<'info>>
	) -> Result<()> {
		handle_update_bid_ask_twap(ctx)
	}

	/**
	 *
	 * ADMIN INSTRUCTIONS
	 *
	 */

	/**
	 *
	 * INITIALIZE
	 *
	 */

	pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
		handle_initialize(ctx)
	}

	/**
	 *
	 * MARKET INSTRUCTIONS
	 *
	 */

	pub fn initialize_market(
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
		base_spread: u32,
		max_spread: u32,
		max_open_interest: u128,
		order_step_size: u64,
		order_tick_size: u64,
		min_order_size: u64,
		concentration_coef_scale: u128,
		curve_update_intensity: u8,
		amm_jit_intensity: u8
	) -> Result<()> {
		handle_initialize_market(
			ctx,
			oracle_source,
			active_status,
			synthetic_tier,
			order_tick_size,
			order_step_size,
			name,
			// perp
			amm_base_asset_reserve,
			amm_quote_asset_reserve,
			amm_periodicity,
			amm_peg_multiplier,
			base_spread,
			max_spread,
			max_open_interest,
			order_step_size,
			order_tick_size,
			min_order_size,
			concentration_coef_scale,
			curve_update_intensit,
			amm_jit_intensity
		)
	}

	pub fn update_market_expiry(
		ctx: Context<AdminUpdateMarket>,
		expiry_ts: i64
	) -> Result<()> {
		handle_update_market_expiry(ctx, expiry_ts)
	}

	pub fn update_market_paused_operations(
		ctx: Context<AdminUpdateMarket>,
		paused_operations: u8
	) -> Result<()> {
		handle_update_market_paused_operations(ctx, paused_operations)
	}

	pub fn update_market_synthetic_tier(
		ctx: Context<AdminUpdateMarket>,
		synthetic_tier: SyntheticTier
	) -> Result<()> {
		handle_update_market_synthetic_tier(ctx, synthetic_tier)
	}

	pub fn update_market_oracle(
		ctx: Context<AdminUpdateMarketOracle>,
		oracle: Pubkey,
		oracle_source: OracleSource
	) -> Result<()> {
		handle_update_market_oracle(ctx, oracle, oracle_source)
	}

	pub fn update_market_status(
		ctx: Context<AdminUpdateMarket>,
		status: MarketStatus
	) -> Result<()> {
		handle_update_market_status(ctx, status)
	}

	pub fn update_market_name(
		ctx: Context<AdminUpdateMarket>,
		name: [u8; 32]
	) -> Result<()> {
		handle_update_market_name(ctx, name)
	}

	pub fn update_market_number_of_users(
		ctx: Context<AdminUpdateMarket>,
		number_of_users: Option<u32>,
		number_of_users_with_base: Option<u32>
	) -> Result<()> {
		handle_update_market_number_of_users(
			ctx,
			number_of_users,
			number_of_users_with_base
		)
	}

	pub fn update_market_fee_adjustment(
		ctx: Context<AdminUpdateMarket>,
		fee_adjustment: i16
	) -> Result<()> {
		handle_update_market_fee_adjustment(ctx, fee_adjustment)
	}

	pub fn update_market_curve_update_intensity(
		ctx: Context<AdminUpdateMarket>,
		curve_update_intensity: u8
	) -> Result<()> {
		handle_update_market_curve_update_intensity(ctx, curve_update_intensity)
	}

	pub fn update_market_target_base_asset_amount_per_lp(
		ctx: Context<AdminUpdateMarket>,
		target_base_asset_amount_per_lp: i32
	) -> Result<()> {
		handle_update_market_target_base_asset_amount_per_lp(
			ctx,
			target_base_asset_amount_per_lp
		)
	}

	/// Send leftover profit from closed market to revenue pool
	pub fn settle_expired_market_pools_to_revenue_pool(
		ctx: Context<SettleExpiredMarketPoolsToRevenuePool>
	) -> Result<()> {
		handle_settle_expired_market_pools_to_revenue_pool(ctx)
	}

	pub fn delete_initialized_market(
		ctx: Context<DeleteInitializedMarket>,
		market_index: u16
	) -> Result<()> {
		handle_delete_initialized_market(ctx, market_index)
	}

	/**
	 *
	 * FEE POOL INSTRUCTIONS
	 *
	 */

	pub fn transfer_fees_to_insurance_fund<'c: 'info, 'info>(
		ctx: Context<'_, '_, 'c, 'info, TransferFeesToInsuranceFund<'info>>,
		market_index: u16
	) -> Result<()> {
		handle_tranfer_fees_to_insurance_fund(ctx, market_index)
	}

	pub fn transfer_fees_to_treasury<'c: 'info, 'info>(
		ctx: Context<'_, '_, 'c, 'info, TransferFeesToTreasury<'info>>,
		market_index: u16
	) -> Result<()> {
		handle_transfer_fees_to_treasury(ctx, market_index)
	}

	pub fn burn_gov_token_with_fees<'c: 'info, 'info>(
		ctx: Context<'_, '_, 'c, 'info, BurnGovTokenWithFees<'info>>,
		market_index: u16
	) -> Result<()> {
		handle_burn_gov_token_with_fees(ctx, market_index)
	}

	/**
	 *
	 * AMM INSTRUCTIONS
	 *
	 */

	pub fn update_amms<'c: 'info, 'info>(
		ctx: Context<'_, '_, 'c, 'info, UpdateAMM<'info>>,
		market_indexes: [u16; 5]
	) -> Result<()> {
		handle_update_amms(ctx, market_indexes)
	}

	pub fn update_market_concentration_coef(
		ctx: Context<AdminUpdateMarket>,
		concentration_scale: u128
	) -> Result<()> {
		handle_update_market_concentration_coef(ctx, concentration_scale)
	}

	pub fn update_market_max_spread(
		ctx: Context<AdminUpdateMarket>,
		max_spread: u32
	) -> Result<()> {
		handle_update_market_max_spread(ctx, max_spread)
	}

	pub fn update_market_base_spread(
		ctx: Context<AdminUpdateMarket>,
		base_spread: u32
	) -> Result<()> {
		handle_update_market_base_spread(ctx, base_spread)
	}

	pub fn update_market_min_order_size(
		ctx: Context<AdminUpdateMarket>,
		order_size: u64
	) -> Result<()> {
		handle_update_market_min_order_size(ctx, order_size)
	}

	pub fn update_market_step_size_and_tick_size(
		ctx: Context<AdminUpdateMarket>,
		step_size: u64,
		tick_size: u64
	) -> Result<()> {
		handle_update_market_step_size_and_tick_size(ctx, step_size, tick_size)
	}

	pub fn update_market_max_slippage_ratio(
		ctx: Context<AdminUpdateMarket>,
		max_slippage_ratio: u16
	) -> Result<()> {
		handle_update_market_max_slippage_ratio(ctx, max_slippage_ratio)
	}

	pub fn update_market_max_fill_reserve_fraction(
		ctx: Context<AdminUpdateMarket>,
		max_fill_reserve_fraction: u16
	) -> Result<()> {
		handle_update_market_max_fill_reserve_fraction(
			ctx,
			max_fill_reserve_fraction
		)
	}

	pub fn update_market_max_open_interest(
		ctx: Context<AdminUpdateMarket>,
		max_open_interest: u128
	) -> Result<()> {
		handle_update_market_max_open_interest(ctx, max_open_interest)
	}

	pub fn update_amm_jit_intensity(
		ctx: Context<AdminUpdateMarket>,
		amm_jit_intensity: u8
	) -> Result<()> {
		handle_update_amm_jit_intensity(ctx, amm_jit_intensity)
	}

	pub fn move_amm_price(
		ctx: Context<AdminUpdateMarket>,
		base_asset_reserve: u128,
		quote_asset_reserve: u128,
		sqrt_k: u128
	) -> Result<()> {
		handle_move_amm_price(ctx, base_asset_reserve, quote_asset_reserve, sqrt_k)
	}

	pub fn recenter_market_amm(
		ctx: Context<AdminUpdateMarket>,
		peg_multiplier: u128,
		sqrt_k: u128
	) -> Result<()> {
		handle_recenter_market_amm(ctx, peg_multiplier, sqrt_k)
	}

	pub fn repeg_amm_curve(
		ctx: Context<RepegCurve>,
		new_peg_candidate: u128
	) -> Result<()> {
		handle_repeg_amm_curve(ctx, new_peg_candidate)
	}

	pub fn update_k(ctx: Context<AdminUpdateK>, sqrt_k: u128) -> Result<()> {
		handle_update_k(ctx, sqrt_k)
	}

	pub fn update_market_amm_summary_stats(
		ctx: Context<AdminUpdateMarketAmmSummaryStats>,
		params: UpdateMarketSummaryStatsParams
	) -> Result<()> {
		handle_update_market_amm_summary_stats(ctx, params)
	}

	pub fn update_market_amm_oracle_twap(ctx: Context<RepegCurve>) -> Result<()> {
		handle_update_amm_oracle_twap(ctx)
	}

	pub fn reset_market_amm_oracle_twap(ctx: Context<RepegCurve>) -> Result<()> {
		handle_reset_amm_oracle_twap(ctx)
	}

	/**
	 *
	 * STATE INSTUCTIONS (ADMIN)
	 *
	 */

	pub fn update_admin(
		ctx: Context<AdminUpdateState>,
		admin: Pubkey
	) -> Result<()> {
		handle_update_admin(ctx, admin)
	}

	pub fn update_fee_structure(
		ctx: Context<AdminUpdateState>,
		fee_structure: FeeStructure
	) -> Result<()> {
		handle_update_fee_structure(ctx, fee_structure)
	}

	pub fn update_lp_cooldown_time(
		ctx: Context<AdminUpdateState>,
		lp_cooldown_time: u64
	) -> Result<()> {
		handle_update_lp_cooldown_time(ctx, lp_cooldown_time)
	}

	pub fn update_whitelist_mint(
		ctx: Context<AdminUpdateState>,
		whitelist_mint: Pubkey
	) -> Result<()> {
		handle_update_whitelist_mint(ctx, whitelist_mint)
	}

	pub fn update_discount_mint(
		ctx: Context<AdminUpdateState>,
		discount_mint: Pubkey
	) -> Result<()> {
		handle_update_discount_mint(ctx, discount_mint)
	}

	pub fn update_exchange_status(
		ctx: Context<AdminUpdateState>,
		exchange_status: u8
	) -> Result<()> {
		handle_update_exchange_status(ctx, exchange_status)
	}

	pub fn update_auction_duration(
		ctx: Context<AdminUpdateState>,
		min_auction_duration: u8
	) -> Result<()> {
		handle_update_auction_duration(ctx, min_auction_duration)
	}

	pub fn update_state_settlement_duration(
		ctx: Context<AdminUpdateState>,
		settlement_duration: u16
	) -> Result<()> {
		handle_update_state_settlement_duration(ctx, settlement_duration)
	}

	pub fn update_state_max_number_of_sub_accounts(
		ctx: Context<AdminUpdateState>,
		max_number_of_sub_accounts: u16
	) -> Result<()> {
		handle_update_state_max_number_of_sub_accounts(
			ctx,
			max_number_of_sub_accounts
		)
	}

	pub fn update_state_max_initialize_user_fee(
		ctx: Context<AdminUpdateState>,
		max_initialize_user_fee: u16
	) -> Result<()> {
		handle_update_state_max_initialize_user_fee(ctx, max_initialize_user_fee)
	}

	/**
	 *
	 * INDEX FUND INSTUCTIONS
	 *
	 */

	pub fn initialize_index_fund<'c: 'info, 'info>(
		ctx: Context<'_, '_, 'c, 'info, InitializeIndexFund<'info>>,
		name: [u8; 32],
		public: bool
	) -> Result<()> {
		handle_initialize_index_fund(ctx, name, public)
	}

	pub fn update_index_fund_visibility<'c: 'info, 'info>(
		ctx: Context<'_, '_, 'c, 'info, UpdateIndexFund<'info>>,
		public: bool
	) -> Result<()> {
		handle_update_index_fund_visibility(ctx, public)
	}

	pub fn update_index_fund_assets<'c: 'info, 'info>(
		ctx: Context<'_, '_, 'c, 'info, UpdateIndexFund<'info>>,
		assets: IndexFundAssets
	) -> Result<()> {
		handle_update_index_fund_assets(ctx, assets)
	}

	pub fn rebalance_index_fund<'c: 'info, 'info>(
		ctx: Context<'_, '_, 'c, 'info, RebalanceIndexFund<'info>>,
		market_index: u16
	) -> Result<()> {
		handle_rebalance_index_fund(ctx, market_index)
	}

	/**
	 *
	 * ORACLE INSTUCTIONS (ADMIN)
	 *
	 */

	pub fn initialize_pyth_pull_oracle(
		ctx: Context<InitPythPullPriceFeed>,
		feed_id: [u8; 32]
	) -> Result<()> {
		handle_initialize_pyth_pull_oracle(ctx, feed_id)
	}

	pub fn update_pyth_pull_oracle(
		ctx: Context<UpdatePythPullOraclePriceFeed>,
		feed_id: [u8; 32],
		params: Vec<u8>
	) -> Result<()> {
		handle_update_pyth_pull_oracle(ctx, feed_id, params)
	}

	pub fn post_pyth_pull_oracle_update_atomic(
		ctx: Context<PostPythPullOracleUpdateAtomic>,
		feed_id: [u8; 32],
		params: Vec<u8>
	) -> Result<()> {
		handle_post_pyth_pull_oracle_update_atomic(ctx, feed_id, params)
	}

	pub fn post_multi_pyth_pull_oracle_updates_atomic<'c: 'info, 'info>(
		ctx: Context<
			'_,
			'_,
			'c,
			'info,
			PostPythPullMultiOracleUpdatesAtomic<'info>
		>,
		params: Vec<u8>
	) -> Result<()> {
		handle_post_multi_pyth_pull_oracle_updates_atomic(ctx, params)
	}

	/**
	 *
	 * INSURANCE INSTUCTIONS (ADMIN)
	 *
	 */

	pub fn initialize_insurance_fund(
		ctx: Context<InitializeInsuranceFund>,
		insurance_fund_total_factor: u32
	) -> Result<()> {
		handle_initialize_insurance_fund(ctx, insurance_fund_total_factor)
	}

	pub fn update_insurance_fund_factor(
		ctx: Context<AdminUpdateInsuranceFund>,
		user_insurance_fund_factor: u32,
		total_insurance_fund_factor: u32
	) -> Result<()> {
		handle_update_insurance_fund_factor(
			ctx,
			user_insurance_fund_factor,
			total_insurance_fund_factor
		)
	}

	pub fn update_insurance_fund_paused_operations(
		ctx: Context<AdminUpdateInsuranceFund>,
		paused_operations: u8
	) -> Result<()> {
		handle_update_insurance_fund_paused_operations(ctx, paused_operations)
	}

	pub fn initialize_protocol_insurance_fund_shares_transfer_config(
		ctx: Context<InitializeProtocolInsuranceFundSharesTransferConfig>
	) -> Result<()> {
		handle_initialize_protocol_insurance_fund_shares_transfer_config(ctx)
	}

	pub fn update_protocol_insurance_fund_shares_transfer_config(
		ctx: Context<UpdateProtocolInsuranceFundSharesTransferConfig>,
		whitelisted_signers: Option<[Pubkey; 4]>,
		max_transfer_per_epoch: Option<u128>
	) -> Result<()> {
		handle_update_protocol_insurance_fund_shares_transfer_config(
			ctx,
			whitelisted_signers,
			max_transfer_per_epoch
		)
	}

	/**
	 *
	 * INSURANCE FUND STAKER INSTUCTIONS (ADMIN)
	 *
	 */

	pub fn initialize_insurance_fund_stake(
		ctx: Context<InitializeInsuranceFundStake>
	) -> Result<()> {
		handle_initialize_insurance_fund_stake(ctx)
	}

	pub fn add_insurance_fund_stake<'c: 'info, 'info>(
		ctx: Context<'_, '_, 'c, 'info, AddInsuranceFundStake<'info>>,
		amount: u64
	) -> Result<()> {
		handle_add_insurance_fund_stake(ctx, amount)
	}

	pub fn request_remove_insurance_fund_stake(
		ctx: Context<RequestRemoveInsuranceFundStake>,
		amount: u64
	) -> Result<()> {
		handle_request_remove_insurance_fund_stake(ctx, amount)
	}

	pub fn cancel_request_remove_insurance_fund_stake(
		ctx: Context<RequestRemoveInsuranceFundStake>
	) -> Result<()> {
		handle_cancel_request_remove_insurance_fund_stake(ctx)
	}

	pub fn remove_insurance_fund_stake<'c: 'info, 'info>(
		ctx: Context<'_, '_, 'c, 'info, RemoveInsuranceFundStake<'info>>
	) -> Result<()> {
		handle_remove_insurance_fund_stake(ctx)
	}

	pub fn transfer_protocol_insurance_fund_shares(
		ctx: Context<TransferProtocolInsuranceFundShares>,
		shares: u128
	) -> Result<()> {
		handle_transfer_protocol_insurance_fund_shares(ctx, shares)
	}
}

#[cfg(not(feature = "no-entrypoint"))]
use solana_security_txt::security_txt;
#[cfg(not(feature = "no-entrypoint"))]
security_txt! {
    name: "Normal v1",
    project_url: "https://normalfinance.io",
    contacts: "link:https://docs.normalfinance.io/security/bug-bounty",
    policy: "https://github.com/normalfinance/normal-v1/blob/main/SECURITY.md",
    preferred_languages: "en",
    source_code: "https://github.com/normalfinance/normal-v1"
}
