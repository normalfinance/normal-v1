#![allow(clippy::too_many_arguments)]
#![allow(clippy::bool_assert_comparison)]
#![allow(clippy::comparison_chain)]

use anchor_lang::prelude::*;

#[cfg(test)]
use math::amm;
use math::{ bn, constants::* };
use state::oracle::OracleSource;

use crate::state::market::{ SyntheticTier, MarketStatus };
use crate::state::state::FeeStructure;
use crate::state::state::*;

pub mod constants;
pub mod controller;
pub mod error;
pub mod ids;
pub mod instructions;
pub mod macros;
pub mod math;
mod signer;
pub mod security;
pub mod state;
pub mod util;
#[cfg(test)]
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
	 * State/admin instructions
	 *
	 */

	pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
		handle_initialize(ctx)
	}

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

	/**
	 * Collateral Instructions
	 */

	pub fn add_collateral_type(ctx: Context<AdminUpdateState>) -> Result<()> {
		handle_add_collateral_type(ctx)
	}

	pub fn remove_collateral_type(ctx: Context<AdminUpdateState>) -> Result<()> {
		handle_remove_collateral_type(ctx)
	}

	/**
	 * Vaults Config Instructions
	 */

	pub fn initialize_vaults_config(
		ctx: Context<AdminUpdateState>
	) -> Result<()> {
		handle_initialize_vaults_config(ctx)
	}

	/**
	 *
	 * AMM Instructions (admin)
	 *
	 */

	/// Initializes a market's AMM account.
	/// Fee rate is set to the default values on the config and supplied fee_tier.
	///
	/// ### Parameters
	/// - `bumps` - The bump value when deriving the PDA of the AMM address.
	/// - `tick_spacing` - The desired tick spacing for this pool.
	///
	/// #### Special Errors
	/// `InvalidTokenMintOrder` - The order of mints have to be ordered by
	///
	pub fn initialize_amm(
		ctx: Context<InitializeAMM>,
		bumps: AMMBumps,
		tick_spacing: u16
	) -> Result<()> {
		handle_initialize_amm(ctx, bumps, tick_spacing)
	}

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

	/// Set the AMM reward authority at the provided `reward_index`.
	/// Only the current reward authority for this reward index has permission to invoke this instruction.
	///
	/// ### Authority
	/// - "reward_authority" - Set authority that can control reward emission for this particular reward.
	///
	/// #### Special Errors
	/// - `InvalidRewardIndex` - If the provided reward index doesn't match the lowest uninitialized
	///                          index in this pool, or exceeds NUM_REWARDS, or
	///                          all reward slots for this pool has been initialized.
	pub fn set_amm_reward_authority(
		ctx: Context<SetAMMRewardAuthority>,
		reward_index: u8
	) -> Result<()> {
		handle_set_amm_reward_authority(ctx, reward_index)
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
	/// - `SqrtPriceOutOfBounds` - User provided parameter `sqrt_price_limit` is over Whirlppool's max/min bounds for sqrt-price.
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
		bumps: OpenPositionBumps,
		tick_lower_index: i32,
		tick_upper_index: i32
	) -> Result<()> {
		handle_open_liquidity_position(
			ctx,
			bumps,
			tick_lower_index,
			tick_upper_index
		)
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
		bumps: OpenPositionWithMetadataBumps,
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
		token_max_quote: u64
	) -> Result<()> {
		handle_increase_liquidity(ctx, liquidity_amount, token_max_quote)
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
		token_min_quote: u64
	) -> Result<()> {
		handle_decrease_liquidity(ctx, liquidity_amount, token_min_quote)
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

	pub fn update_market_synthetic_tier(
		ctx: Context<AdminUpdateMarket>,
		synthetic_tier: SyntheticTier
	) -> Result<()> {
		handle_update_market_synthetic_tier(ctx, synthetic_tier)
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

	pub fn update_market_amm_oracle_twap(ctx: Context<RepegCurve>) -> Result<()> {
		handle_update_amm_oracle_twap(ctx)
	}

	pub fn reset_market_amm_oracle_twap(ctx: Context<RepegCurve>) -> Result<()> {
		handle_reset_amm_oracle_twap(ctx)
	}

	/**
	 *
	 * Fund Instructions
	 *
	 */

	pub fn initialize_fund<'c: 'info, 'info>(
		ctx: Context<'_, '_, 'c, 'info, InitializeIndexFund<'info>>,
		name: [u8; 32],
		public: bool
	) -> Result<()> {
		handle_initialize_fund(ctx, name, public)
	}

	pub fn update_fund_visibility<'c: 'info, 'info>(
		ctx: Context<'_, '_, 'c, 'info, UpdateIndexFund<'info>>,
		public: bool
	) -> Result<()> {
		handle_update_fund_visibility(ctx, public)
	}

	pub fn update_fund_assets<'c: 'info, 'info>(
		ctx: Context<'_, '_, 'c, 'info, UpdateIndexFund<'info>>,
		assets: IndexFundAssets
	) -> Result<()> {
		handle_update_fund_assets(ctx, assets)
	}

	pub fn rebalance_fund<'c: 'info, 'info>(
		ctx: Context<'_, '_, 'c, 'info, RebalanceIndexFund<'info>>,
		market_index: u16
	) -> Result<()> {
		handle_rebalance_fund(ctx, market_index)
	}

	/**
	 *
	 * Oracle Instructions
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
}
