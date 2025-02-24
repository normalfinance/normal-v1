#![allow(clippy::too_many_arguments)]
#![allow(clippy::bool_assert_comparison)]
#![allow(clippy::comparison_chain)]

use anchor_lang::prelude::*;

use crate::state::state::*;

pub mod controller;
pub mod errors;
pub mod ids;
pub mod instructions;
pub mod macros;
pub mod math;
mod signer;
pub mod security;
pub mod state;

use instructions::*;

#[cfg(test)]
mod test_utils;
mod validation;

declare_id!("BqxukimGxeWzUJSwpDyGoc6Q78iMtAhwSYxgiq2vXTxB");

#[program]
pub mod normal {
	use super::*;

	// State instructions

	pub fn initialize(
		ctx: Context<Initialize>,
		total_debt_ceiling: u64
	) -> Result<()> {
		handle_initialize_state(ctx, total_debt_ceiling)
	}

	pub fn update_state_admin(
		ctx: Context<AdminUpdateState>,
		admin: Pubkey
	) -> Result<()> {
		handle_update_state_admin(ctx, admin)
	}

	pub fn update_state_initial_pct_to_liquidate(
		ctx: Context<AdminUpdateState>,
		initial_pct_to_liquidate: u16
	) -> Result<()> {
		handle_update_state_initial_pct_to_liquidate(ctx, initial_pct_to_liquidate)
	}

	pub fn update_state_liquidation_duration(
		ctx: Context<AdminUpdateState>,
		liquidation_duration: u8
	) -> Result<()> {
		handle_update_state_liquidation_duration(ctx, liquidation_duration)
	}

	pub fn update_state_liquidation_margin_buffer_ratio(
		ctx: Context<AdminUpdateState>,
		liquidation_margin_buffer_ratio: u32
	) -> Result<()> {
		handle_update_state_liquidation_margin_buffer_ratio(
			ctx,
			liquidation_margin_buffer_ratio
		)
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

	// Synth Market instructions

	pub fn initialize_synth_market(
		ctx: Context<InitializeSynthMarket>,
		tick_spacing: u16,
		initial_sqrt_price: u128,
		oracle_source: OracleSource,
		fee_rate: u16,
		protocol_fee_rate: u16,
		max_price_variance: u16
	) -> Result<()> {
		handle_initialize_synth_market(ctx)
	}

	pub fn initialize_synth_market_shutdown(
		ctx: Context<AdminUpdateSynthMarket>,
		expiry_ts: i64
	) -> Result<()> {
		handle_initialize_synth_market_shutdown(ctx, expiry_ts)
	}

	pub fn delete_initialized_synth_market(
		ctx: Context<DeleteInitializedSynthMarket>,
		market_index: u16
	) -> Result<()> {
		handle_delete_initialized_synth_market(ctx, market_index)
	}

	pub fn update_synth_market_debt_ceiling(
		ctx: Context<AdminUpdateSynthMarket>,
		debt_ceiling: u128
	) -> Result<()> {
		handle_update_synth_market_debt_celing(ctx, debt_ceiling)
	}

	pub fn update_synth_market_debt_floor(
		ctx: Context<AdminUpdateSynthMarket>,
		debt_floor: u32
	) -> Result<()> {
		handle_update_synth_market_debt_celing(ctx, debt_floor)
	}

	pub fn update_synth_market_expiry(
		ctx: Context<AdminUpdateSynthMarket>,
		expiry_ts: i64
	) -> Result<()> {
		handle_update_synth_market_expiry(ctx, expiry_ts)
	}

	pub fn update_synth_market_imf_factor(
		ctx: Context<AdminUpdateSynthMarket>,
		imf_factor: u32
	) -> Result<()> {
		handle_update_synth_market_imf_factor(ctx, imf_factor)
	}

	pub fn update_synth_market_liquidation_fee(
		ctx: Context<AdminUpdateSynthMarket>,
		liquidator_fee: u32,
		insurance_fund_liquidation_fee: u32
	) -> Result<()> {
		handle_update_synth_market_liquidation_fee(
			ctx,
			liquidator_fee,
			insurance_fund_liquidation_fee
		)
	}

	pub fn update_synth_market_liquidation_penalty(
		ctx: Context<AdminUpdateSynthMarket>,
		liquidator_penalty: u32
	) -> Result<()> {
		handle_update_synth_market_liquidation_penalty(ctx, liquidator_penalty)
	}

	pub fn update_synth_market_margin_ratio(
		ctx: Context<AdminUpdatSyntheMarket>,
		margin_ratio_initial: u32,
		margin_ratio_maintenance: u32
	) -> Result<()> {
		handle_update_synth_market_margin_ratio(
			ctx,
			margin_ratio_initial,
			margin_ratio_maintenance
		)
	}

	pub fn update_synth_market_name(
		ctx: Context<AdminUpdateSynthMarket>,
		name: [u8; 32]
	) -> Result<()> {
		handle_update_synth_market_name(ctx, name)
	}

	pub fn update_synth_market_number_of_users(
		ctx: Context<AdminUpdateSynthMarket>,
		number_of_users: Option<u32>
	) -> Result<()> {
		handle_update_synth_market_number_of_users(ctx, number_of_users)
	}

	// Synth Market - Collateral instructions

	pub fn deposit_collatarel<'c: 'info, 'info>(
		ctx: Context<'_, '_, 'c, 'info, DepositCollateral<'info>>,
		market_index: u16,
		amount: u64,
		reduce_only: bool
	) -> Result<()> {
		handle_deposit_collateral(ctx, market_index, amount, reduce_only)
	}

	pub fn transfer_collateral<'c: 'info, 'info>(
		ctx: Context<'_, '_, 'c, 'info, TransferCollateral<'info>>,
		market_index: u16,
		amount: u64
	) -> anchor_lang::Result<()> {
		handle_transfer_collateral(ctx, market_index, amount)
	}

	pub fn withdraw_collateral<'c: 'info, 'info>(
		ctx: Context<'_, '_, 'c, 'info, WithdrawCollateral<'info>>,
		market_index: u16,
		amount: u64,
		reduce_only: bool
	) -> anchor_lang::Result<()> {
		handle_withdraw_collateral(ctx, market_index, amount, reduce_only)
	}

	// User instructions

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

	pub fn update_user_idle<'c: 'info, 'info>(
		ctx: Context<'_, '_, 'c, 'info, UpdateUserIdle<'info>>
	) -> Result<()> {
		handle_update_user_idle(ctx)
	}

	pub fn update_user_reduce_only(
		ctx: Context<UpdateUser>,
		_sub_account_id: u16,
		reduce_only: bool
	) -> Result<()> {
		handle_update_user_reduce_only(ctx, _sub_account_id, reduce_only)
	}

	pub fn update_user_custom_margin_ratio(
		ctx: Context<UpdateUser>,
		_sub_account_id: u16,
		margin_ratio: u32
	) -> Result<()> {
		handle_update_user_custom_margin_ratio(ctx, _sub_account_id, margin_ratio)
	}

	pub fn delete_user<'c: 'info, 'info>(
		ctx: Context<'_, '_, 'c, 'info, DeleteUser>
	) -> Result<()> {
		handle_delete_user(ctx)
	}

	pub fn reclaim_rent(ctx: Context<ReclaimRent>) -> Result<()> {
		handle_reclaim_rent(ctx)
	}

	// AMM instructions

	/// Initializes a tick_array account to represent a tick-range in an AMM.
	///
	/// ### Parameters
	/// - `start_tick_index` - The starting tick index for this tick-array.
	///                        Has to be a multiple of TickArray size & the tick spacing of this pool.
	///
	/// #### Special Errors
	/// - `InvalidStartTick` - if the provided start tick is out of bounds or is not a multiple of
	///                        TICK_ARRAY_SIZE * tick spacing.
	pub fn initialize_amm_tick_array(
		ctx: Context<InitializeAMMTickArray>,
		start_tick_index: i32
	) -> Result<()> {
		handle_initialize_amm_tick_array(ctx, start_tick_index)
	}

	/// Initialize reward for an AMM. An AMM can only support up to a set number of rewards.
	///
	/// ### Authority
	/// - "reward_authority" - assigned authority by the reward_super_authority for the specified
	///                        reward-index in this AMM
	///
	/// ### Parameters
	/// - `reward_index` - The reward index that we'd like to initialize. (0 <= index <= NUM_REWARDS)
	///
	/// #### Special Errors
	/// - `InvalidRewardIndex` - If the provided reward index doesn't match the lowest uninitialized
	///                          index in this pool, or exceeds NUM_REWARDS, or
	///                          all reward slots for this pool has been initialized.
	pub fn initialize_amm_reward(
		ctx: Context<InitializeAMMReward>,
		reward_index: u8
	) -> Result<()> {
		handle_initialize_amm_reward(ctx, reward_index)
	}

	/// Set the reward emissions for a reward in an AMM.
	///
	/// ### Authority
	/// - "reward_authority" - assigned authority by the reward_super_authority for the specified
	///                        reward-index in this AMM
	///
	/// ### Parameters
	/// - `reward_index` - The reward index (0 <= index <= NUM_REWARDS) that we'd like to modify.
	/// - `emissions_per_second_x64` - The amount of rewards emitted in this pool.
	///
	/// #### Special Errors
	/// - `RewardVaultAmountInsufficient` - The amount of rewards in the reward vault cannot emit
	///                                     more than a day of desired emissions.
	/// - `InvalidTimestamp` - Provided timestamp is not in order with the previous timestamp.
	/// - `InvalidRewardIndex` - If the provided reward index doesn't match the lowest uninitialized
	///                          index in this pool, or exceeds NUM_REWARDS, or
	///                          all reward slots for this pool has been initialized.
	pub fn set_amm_reward_emissions(
		ctx: Context<SetAMMRewardEmissions>,
		reward_index: u8,
		emissions_per_second_x64: u128
	) -> Result<()> {
		handle_set_amm_reward_emissions(ctx, reward_index, emissions_per_second_x64)
	}

	/// Sets the fee rate for an AMM.
	/// Fee rate is represented as hundredths of a basis point.
	/// Only the current fee authority has permission to invoke this instruction.
	///
	/// ### Authority
	/// - "fee_authority" - Set authority that can modify pool fees in the AMMConfig
	///
	/// ### Parameters
	/// - `fee_rate` - The rate that the pool will use to calculate fees going onwards.
	///
	/// #### Special Errors
	/// - `FeeRateMaxExceeded` - If the provided fee_rate exceeds MAX_FEE_RATE.
	pub fn set_amm_fee_rate(
		ctx: Context<SetAMMFeeRate>,
		fee_rate: u16
	) -> Result<()> {
		handle_set_amm_fee_rate(ctx, fee_rate)
	}

	/// Sets the protocol fee rate for an AMM.
	/// Protocol fee rate is represented as a basis point.
	/// Only the current fee authority has permission to invoke this instruction.
	///
	/// ### Authority
	/// - "fee_authority" - Set authority that can modify pool fees in the AMMConfig
	///
	/// ### Parameters
	/// - `protocol_fee_rate` - The rate that the pool will use to calculate protocol fees going onwards.
	///
	/// #### Special Errors
	/// - `ProtocolFeeRateMaxExceeded` - If the provided default_protocol_fee_rate exceeds MAX_PROTOCOL_FEE_RATE.
	pub fn set_amm_protocol_fee_rate(
		ctx: Context<SetAMMProtocolFeeRate>,
		protocol_fee_rate: u16
	) -> Result<()> {
		handle_set_amm_protocol_fee_rate(ctx, protocol_fee_rate)
	}

	/**
	 *
	 *
	 * AMM INSTRUCTIONS (user)
	 *
	 */

	/// Perform a swap in this AMM
	///
	/// ### Authority
	/// - "token_authority" - The authority to withdraw tokens from the input token account.
	///
	/// ### Parameters
	/// - `amount` - The amount of input or output token to swap from (depending on amount_specified_is_input).
	/// - `other_amount_threshold` - The maximum/minimum of input/output token to swap into (depending on amount_specified_is_input).
	/// - `sqrt_price_limit` - The maximum/minimum price the swap will swap to.
	/// - `amount_specified_is_input` - Specifies the token the parameter `amount`represents. If true, the amount represents the input token of the swap.
	/// - `a_to_b` - The direction of the swap. True if swapping from A to B. False if swapping from B to A.
	///
	/// #### Special Errors
	/// - `ZeroTradableAmount` - User provided parameter `amount` is 0.
	/// - `InvalidSqrtPriceLimitDirection` - User provided parameter `sqrt_price_limit` does not match the direction of the trade.
	/// - `SqrtPriceOutOfBounds` - User provided parameter `sqrt_price_limit` is over AMM's max/min bounds for sqrt-price.
	/// - `InvalidTickArraySequence` - User provided tick-arrays are not in sequential order required to proceed in this trade direction.
	/// - `TickArraySequenceInvalidIndex` - The swap loop attempted to access an invalid array index during the query of the next initialized tick.
	/// - `TickArrayIndexOutofBounds` - The swap loop attempted to access an invalid array index during tick crossing.
	/// - `LiquidityOverflow` - Liquidity value overflowed 128bits during tick crossing.
	/// - `InvalidTickSpacing` - The swap pool was initialized with tick-spacing of 0.
	pub fn swap(
		ctx: Context<Swap>,
		amount: u64,
		other_amount_threshold: u64,
		sqrt_price_limit: u128,
		amount_specified_is_input: bool,
		a_to_b: bool
	) -> Result<()> {
		handle_swap(
			ctx,
			amount,
			other_amount_threshold,
			sqrt_price_limit,
			amount_specified_is_input,
			a_to_b
		)
	}

	/// Open a position in an AMM. A unique token will be minted to represent the position
	/// in the users wallet. The position will start off with 0 liquidity.
	///
	/// ### Parameters
	/// - `tick_lower_index` - The tick specifying the lower end of the position range.
	/// - `tick_upper_index` - The tick specifying the upper end of the position range.
	///
	/// #### Special Errors
	/// - `InvalidTickIndex` - If a provided tick is out of bounds, out of order or not a multiple of
	///                        the tick-spacing in this pool.
	pub fn open_liquidity_position(
		ctx: Context<OpenLiquidityPosition>,
		tick_lower_index: i32,
		tick_upper_index: i32
	) -> Result<()> {
		handle_open_liquidity_position(ctx, tick_lower_index, tick_upper_index)
	}

	/// Open a position in an AMM. A unique token will be minted to represent the position
	/// in the users wallet. Additional Metaplex metadata is appended to identify the token.
	/// The position will start off with 0 liquidity.
	///
	/// ### Parameters
	/// - `tick_lower_index` - The tick specifying the lower end of the position range.
	/// - `tick_upper_index` - The tick specifying the upper end of the position range.
	///
	/// #### Special Errors
	/// - `InvalidTickIndex` - If a provided tick is out of bounds, out of order or not a multiple of
	///                        the tick-spacing in this pool.
	pub fn open_liquidity_position_with_metadata(
		ctx: Context<OpenPositionWithMetadata>,
		bumps: OpenLiquidityPositionWithMetadataBumps,
		tick_lower_index: i32,
		tick_upper_index: i32
	) -> Result<()> {
		handle_open_liquidity_position_with_metadata(
			ctx,
			bumps,
			tick_lower_index,
			tick_upper_index
		)
	}

	/// Add liquidity to a position in the AMM. This call also updates the position's accrued fees and rewards.
	///
	/// ### Authority
	/// - `position_authority` - authority that owns the token corresponding to this desired position.
	///
	/// ### Parameters
	/// - `liquidity_amount` - The total amount of Liquidity the user is willing to deposit.
	/// - `token_max_quote` - The maximum amount of tokenB the user is willing to deposit.
	///
	/// #### Special Errors
	/// - `LiquidityZero` - Provided liquidity amount is zero.
	/// - `LiquidityTooHigh` - Provided liquidity exceeds u128::max.
	/// - `TokenMaxExceeded` - The required token to perform this operation exceeds the user defined amount.
	pub fn increase_liquidity(
		ctx: Context<ModifyLiquidity>,
		liquidity_amount: u128,
		token_max_synthetic: u64,
		token_max_quote: u64
	) -> Result<()> {
		handle_increase_liquidity(
			ctx,
			liquidity_amount,
			token_max_synthetic,
			token_max_quote
		)
	}

	/// Withdraw liquidity from a position in the AMM. This call also updates the position's accrued fees and rewards.
	///
	/// ### Authority
	/// - `position_authority` - authority that owns the token corresponding to this desired position.
	///
	/// ### Parameters
	/// - `liquidity_amount` - The total amount of Liquidity the user desires to withdraw.
	/// - `token_min_quote` - The minimum amount of tokenB the user is willing to withdraw.
	///
	/// #### Special Errors
	/// - `LiquidityZero` - Provided liquidity amount is zero.
	/// - `LiquidityTooHigh` - Provided liquidity exceeds u128::max.
	/// - `TokenMinSubceeded` - The required token to perform this operation subceeds the user defined amount.
	pub fn decrease_liquidity(
		ctx: Context<ModifyLiquidity>,
		liquidity_amount: u128,
		token_min_a: u64,
		token_min_b: u64
	) -> Result<()> {
		handle_decrease_liquidity(ctx, liquidity_amount, token_min_a, token_min_b)
	}

	/// Update the accrued fees and rewards for a position.
	///
	/// #### Special Errors
	/// - `TickNotFound` - Provided tick array account does not contain the tick for this position.
	/// - `LiquidityZero` - Position has zero liquidity and therefore already has the most updated fees and reward values.
	pub fn update_amm_fees_and_rewards(
		ctx: Context<UpdateFeesAndRewards>
	) -> Result<()> {
		handle_update_amm_fees_and_rewards(ctx)
	}

	/// Collect fees accrued for this position.
	///
	/// ### Authority
	/// - `position_authority` - authority that owns the token corresponding to this desired position.
	pub fn collect_liquidity_position_fees(
		ctx: Context<CollectLiquidityPositionFees>
	) -> Result<()> {
		handle_collect_liquidity_position_fees(ctx)
	}

	/// Collect rewards accrued for this position.
	///
	/// ### Authority
	/// - `position_authority` - authority that owns the token corresponding to this desired position.
	pub fn collect_liquidity_position_reward(
		ctx: Context<CollectLiquidityPositionReward>,
		reward_index: u8
	) -> Result<()> {
		handle_collect_liquidity_position_reward(ctx, reward_index)
	}

	/// Close a position in an AMM. Burns the position token in the owner's wallet.
	///
	/// ### Authority
	/// - "position_authority" - The authority that owns the position token.
	///
	/// #### Special Errors
	/// - `ClosePositionNotEmpty` - The provided position account is not empty.
	pub fn close_liquidity_position(
		ctx: Context<CloseLiquidityPosition>
	) -> Result<()> {
		handle_close_liquidity_position(ctx)
	}

	/// Open a position in an AMM. A unique token will be minted to represent the position
	/// in the users wallet. Additional TokenMetadata extension is initialized to identify the token.
	/// Mint and TokenAccount are based on Token-2022.
	/// The position will start off with 0 liquidity.
	///
	/// ### Parameters
	/// - `tick_lower_index` - The tick specifying the lower end of the position range.
	/// - `tick_upper_index` - The tick specifying the upper end of the position range.
	/// - `with_token_metadata_extension` - If true, the token metadata extension will be initialized.
	///
	/// #### Special Errors
	/// - `InvalidTickIndex` - If a provided tick is out of bounds, out of order or not a multiple of
	///                        the tick-spacing in this pool.
	pub fn open_liquidity_position_with_token_extensions(
		ctx: Context<OpenLiquidityPositionWithTokenExtensions>,
		tick_lower_index: i32,
		tick_upper_index: i32,
		with_token_metadata_extension: bool
	) -> Result<()> {
		handle_open_liquidity_position_with_token_extensions(
			ctx,
			tick_lower_index,
			tick_upper_index,
			with_token_metadata_extension
		)
	}

	/// Close a position in an AMM. Burns the position token in the owner's wallet.
	/// Mint and TokenAccount are based on Token-2022. And Mint accout will be also closed.
	///
	/// ### Authority
	/// - "position_authority" - The authority that owns the position token.
	///
	/// #### Special Errors
	/// - `ClosePositionNotEmpty` - The provided position account is not empty.
	pub fn close_liquidity_position_with_token_extensions(
		ctx: Context<CloseLiquidityPositionWithTokenExtensions>
	) -> Result<()> {
		handle_close_liquidity_position_with_token_extensions(ctx)
	}

	// Insurance Fund instructions

	pub fn initialize_insurance_fund(
		ctx: Context<InitializeInsuranceFund>,
		if_total_factor: u32
	) -> Result<()> {
		handle_initialize_insurance_fund(ctx, if_total_factor)
	}

	pub fn update_if_max_insurance(
		ctx: Context<AdminUpdateInsurnaceFund>,
		max_insurance: u64
	) -> Result<()> {
		handle_update_if_max_insurance(ctx, max_insurance)
	}

	pub fn update_if_paused_operations(
		ctx: Context<AdminUpdateInsurnaceFund>,
		paused_operations: u8
	) -> Result<()> {
		handle_update_if_paused_operations(ctx, paused_operations)
	}

	pub fn update_if_unstaking_period(
		ctx: Context<AdminUpdateInsurnaceFund>,
		if_unstaking_period: i64
	) -> Result<()> {
		handle_update_if_unstaking_period(ctx, if_unstaking_period)
	}

	// Insurane Fund Staker instructions

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
}
