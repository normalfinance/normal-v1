use anchor_lang::prelude::*;
use borsh::{ BorshDeserialize, BorshSerialize };

use drift_macros::assert_no_slop;
use super::{ amm::AMM, insurance::{ InsuranceClaim }, oracle::OracleSource };

#[derive(
	Clone,
	Copy,
	BorshSerialize,
	BorshDeserialize,
	PartialEq,
	Debug,
	Eq,
	Default
)]
pub enum MarketStatus {
	/// warm up period for initialization, swapping is paused
	#[default]
	Initialized,
	/// all operations allowed
	Active,
	/// swaps only able to reduce liability
	ReduceOnly,
	/// market has determined settlement price and positions are expired must be settled
	Settlement,
	/// market has no remaining participants
	Delisted,
}

#[derive(
	Clone,
	Copy,
	BorshSerialize,
	BorshDeserialize,
	PartialEq,
	Debug,
	Eq,
	PartialOrd,
	Ord,
	Default
)]
pub enum Tier {
	/// max insurance capped at A level
	A,
	/// max insurance capped at B level
	B,
	/// max insurance capped at C level
	C,
	/// no insurance
	Speculative,
	/// no insurance, another tranches below
	#[default]
	HighlySpeculative,
	/// no insurance, only single position allowed
	Isolated,
}

impl Tier {
	pub fn is_as_safe_as(
		&self,
		best_contract: &ContractTier,
		best_asset: &AssetTier
	) -> bool {
		self.is_as_safe_as_contract(best_contract) &&
			self.is_as_safe_as_asset(best_asset)
	}

	pub fn is_as_safe_as_contract(&self, other: &ContractTier) -> bool {
		// Contract Tier A safest
		self <= other
	}
	pub fn is_as_safe_as_asset(&self, other: &AssetTier) -> bool {
		// allow Contract Tier A,B,C to rank above Assets below Collateral status
		if other == &AssetTier::Unlisted {
			true
		} else {
			other >= &AssetTier::Cross && self <= &ContractTier::C
		}
	}
}

// #[assert_no_slop]
#[zero_copy(unsafe)]
#[derive(Debug, AnchorSerialize, AnchorDeserialize, PartialEq, Eq)]
#[repr(C)]
pub struct Collateral {
    // pub symbol: String,
    pub mint: Pubkey,
	/// The vault used to store the market's deposits
	/// The amount in the vault should be equal to or greater than deposits - borrows
	pub vault: Pubkey,
    pub oracle: Pubkey,
    /// the oracle provider information. used to decode/scale the oracle data
    pub oracle_source: OracleSource,
    pub oracle_frozen: bool,
    /// The sum of the balances for collateral deposits across users
    /// precision: SPOT_BALANCE_PRECISION
    pub balance: u128,
    /// The amount of collateral sent/received with the Pool to adjust price
    pub pool_delta_aalance: i128,
    /// 24hr average of deposit token amount
    /// precision: token mint precision
    pub token_twap: u64,
    /// The margin ratio which determines how much collateral is required to open a position
    /// e.g. margin ratio of .1 means a user must have $100 of total collateral to open a $1000 position
    /// precision: MARGIN_PRECISION
    pub margin_ratio_initial: u32,
    /// The margin ratio which determines when a user will be liquidated
    /// e.g. margin ratio of .05 means a user must have $50 of total collateral to maintain a $1000 position
    /// else they will be liquidated
    /// precision: MARGIN_PRECISION
    pub margin_ratio_maintenance: u32,
    /// where collateral auctions should take place (3rd party AMM vs private)
    pub auction_config: Auction,
    /// The max amount of token deposits in this market
    /// 0 if there is no limit
    /// precision: token mint precision
    pub max_token_deposits: u64,
    /// What fraction of max_token_deposits
    /// disabled when 0, 1 => 1/10000 => .01% of max_token_deposits
    /// precision: X/10000
    pub max_token_borrows_fraction: u32,
    /// no withdraw limits/guards when deposits below this threshold
    /// precision: token mint precision
    pub withdraw_guard_threshold: u64,
}

// #[assert_no_slop]
#[zero_copy(unsafe)]
#[derive(Debug, AnchorSerialize, AnchorDeserialize, PartialEq, Eq)]
#[repr(C)]
pub struct Synthetic {
    // pub symbol: Symbol,
    pub mint: Pubkey,
	pub vault: Pubkey,
    /// The synthetic tier determines how much insurance a market can receive, with more speculative markets receiving less insurance
    /// It also influences the order markets can be liquidated, with less speculative markets being liquidated first
    pub tier: Tier,
    /// The sum of the balances for synthetic debts across users
    /// precision: SPOT_BALANCE_PRECISION
    pub balance: u128,
    /// 24hr average of synthetic token amount
    /// precision: token mint precision
    pub token_twap: u64,
    /// The maximum position size
    /// if the limit is 0, there is no limit
    /// precision: token mint precision
    pub max_position_size: u64,
}

#[account(zero_copy(unsafe))]
#[derive(Eq, AnchorSerialize, AnchorDeserialize, PartialEq, Debug)]
#[repr(C)]
pub struct Market {
	/// The market's address. It is a pda of the market index
	pub pubkey: Pubkey,
	pub market_index: u16,
	pub collateral: Collateral,
	pub synthetic: Synthetic,
	/// Encoded display name for the market e.g. BTC-SOL
	pub name: [u8; 32],
	/// The market's token mint's decimals. To from decimals to a precision, 10^decimals
	pub decimals: u32,
	/// Whether a market is active, reduce only, expired, etc
	/// Affects whether users can open/close positions
	pub status: MarketStatus,
	pub paused_operations: u8,
	pub amm: AMM,

	/// The sum of the scaled balances for collateral deposits across users
	/// To convert to the collateral token amount, multiply by the cumulative deposit interest
	/// precision: SPOT_BALANCE_PRECISION
	pub collateral_balance: u128,
	/// The sum of the scaled balances for borrows across users
	/// To convert to the borrow token amount, multiply by the cumulative borrow interest
	/// precision: SPOT_BALANCE_PRECISION
	pub debt_balance: u128,
	/// The cumulative interest earned by depositors
	/// Used to calculate the deposit token amount from the deposit balance
	/// precision: SPOT_CUMULATIVE_INTEREST_PRECISION
	pub cumulative_deposit_interest: u128,
	pub cumulative_lp_interest: u128,
	/// no withdraw limits/guards when deposits below this threshold
	/// precision: token mint precision
	pub withdraw_guard_threshold: u64,
	/// The max amount of token deposits in this market
	/// 0 if there is no limit
	/// precision: token mint precision
	pub max_token_deposits: u64,
	/// 24hr average of deposit token amount
	/// precision: token mint precision
	pub collateral_token_twap: u64,
	/// 24hr average of borrow token amount
	/// precision: token mint precision
	pub debt_token_twap: u64,
	/// 24hr average of utilization
	/// which is debt amount over collateral amount
	/// precision: SPOT_UTILIZATION_PRECISION
	pub utilization_twap: u64,
	/// Last time the cumulative deposit interest was updated
	pub last_interest_ts: u64,
	/// Last time the deposit/borrow/utilization averages were updated
	pub last_twap_ts: u64,
	/// The ts when the market will be expired. Only set if market is in reduce only mode
	pub expiry_ts: i64,
	/// The price at which positions will be settled. Only set if market is expired
	/// precision = PRICE_PRECISION
	pub expiry_price: i64,
	/// The maximum spot position size
	/// if the limit is 0, there is no limit
	/// precision: token mint precision
	pub max_position_size: u64,
	/// Every deposit has a deposit record id. This is the next id to use
	pub next_deposit_record_id: u64,
	/// The initial asset weight used to calculate a deposits contribution to a users initial total collateral
	/// e.g. if the asset weight is .8, $100 of deposits contributes $80 to the users initial total collateral
	/// precision: SPOT_WEIGHT_PRECISION
	pub initial_asset_weight: u32,
	/// The maintenance asset weight used to calculate a deposits contribution to a users maintenance total collateral
	/// e.g. if the asset weight is .9, $100 of deposits contributes $90 to the users maintenance total collateral
	/// precision: SPOT_WEIGHT_PRECISION
	pub maintenance_asset_weight: u32,
	/// The initial liability weight used to calculate a borrows contribution to a users initial margin requirement
	/// e.g. if the liability weight is .9, $100 of borrows contributes $90 to the users initial margin requirement
	/// precision: SPOT_WEIGHT_PRECISION
	pub initial_liability_weight: u32,
	/// The maintenance liability weight used to calculate a borrows contribution to a users maintenance margin requirement
	/// e.g. if the liability weight is .8, $100 of borrows contributes $80 to the users maintenance margin requirement
	/// precision: SPOT_WEIGHT_PRECISION
	pub maintenance_liability_weight: u32,
	/// The initial margin fraction factor. Used to increase margin ratio for large positions
	/// precision: MARGIN_PRECISION
	pub imf_factor: u32,
	// A fee applied to the collateral when the vault is liquidated, incentivizing users to maintain sufficient collateral.
	pub liquidation_penalty: u32,
	/// The fee the liquidator is paid for liquidating a Vault
	/// precision: LIQUIDATOR_FEE_PRECISION
	pub liquidator_fee: u32,
	/// The fee the insurance fund receives from liquidation
	/// precision: LIQUIDATOR_FEE_PRECISION
	pub if_liquidation_fee: u32,
	/// The margin ratio which determines how much collateral is required to open a position
	/// e.g. margin ratio of .1 means a user must have $100 of total collateral to open a $1000 position
	/// precision: MARGIN_PRECISION
	pub margin_ratio_initial: u32,
	/// The margin ratio which determines when a user will be liquidated
	/// e.g. margin ratio of .05 means a user must have $50 of total collateral to maintain a $1000 position
	/// else they will be liquidated
	/// precision: MARGIN_PRECISION
	pub margin_ratio_maintenance: u32,
	/// maximum amount of synthetic tokens that can be minted against the market's collateral
	pub debt_ceiling: u128,
	/// minimum amount of synthetic tokens that can be minted against a user's collateral to avoid inefficiencies
	pub debt_floor: u32,



	// Insurance
	//
	/// The market's claim on the insurance fund
	pub insurance_claim: InsuranceClaim,


	/// Auction Config
	///
	/// where collateral auctions should take place (3rd party AMM vs private)
	// pub collateral_action_config: AuctionConfig,


	pub padding: [u8; 43],
}

impl Default for Market {
	fn default() -> Self {
		Market {
			pubkey: Pubkey::default(),
			market_index: 0,
			name: [0; 32],
			status: MarketStatus::default(),
			tier: Tier::default(),
			paused_operations: 0,

			oracle: Pubkey::default(),
			oracle_source: OracleSource::default(),

			token_mint_collateral: Pubkey::default(),
			token_vault_synthetic: Pubkey::default(),
			token_vault_collateral: Pubkey::default(),

			liquidation_penalty: 0,
			liquidator_fee: 0,
			if_liquidation_fee: 0,
			margin_ratio_initial: 0,
			margin_ratio_maintenance: 0,
			imf_factor: 0,
			debt_ceiling: 0,
			debt_floor: 0,
			collateral_lending_utilization: 0,
			collateral_action_config: AuctionConfig::default(),

			amm: AMM {
				oracle: 0,
				oracle_source,
				historical_oracle_data: HistoricalOracleData::default(),
				last_oracle_conf_pct: 0,
				last_oracle_valid: false,
				last_oracle_normalised_price: 0,
				last_oracle_reserve_price_spread_pct: 0,
				oracle_std: 0,

				tick_spacing,
				tick_spacing_seed: tick_spacing.to_le_bytes(),

				liquidity: 0,
				sqrt_price,
				tick_current_index: tick_index_from_sqrt_price(&sqrt_price),

				fee_rate: 0,
				protocol_fee_rate: 0,

				protocol_fee_owed_a: 0,
				protocol_fee_owed_b: 0,

				token_mint_synthetic,
				token_vault_synthetic,
				fee_growth_global_a: 0,

				token_mint_quote,
				token_vault_quote,
				fee_growth_global_b: 0,

				reward_authority: Pubkey::default(),
				reward_infos: [],
			},

			insurance_claim: InsuranceClaim::default(),

			outstanding_debt: 0,
			protocol_debt: 0,

			expiry_ts: 0,
			expiry_price: 0,

			padding: [0; 43],
		}
	}
}

impl Size for Market {
	const SIZE: usize = 1216; // TODO:
}

impl Market {
	pub fn is_operation_paused(&self, operation: MarketOperation) -> bool {
		MarketOperation::is_operation_paused(self.paused_operations, operation)
	}

	pub fn update_debt_ceiling(&self, debt_ceiling: u64) -> NormalResult<u64> {
		// if debt_ceiling > MAX_PROTOCOL_FEE_RATE {
		// 	return Err(ErrorCode::ProtocolFeeRateMaxExceeded.into());
		// }

		// TODO: check if within rolling window
	}

	pub fn get_margin_ratio(
		&self,
		size: u128,
		margin_type: MarginRequirementType
	) -> NormalResult<u32> {
		if self.status == MarketStatus::Settlement {
			return Ok(0); // no liability weight on size
		}

		let default_margin_ratio = match margin_type {
			MarginRequirementType::Initial => self.margin_ratio_initial,
			// MarginRequirementType::Fill => {
			// 	self.margin_ratio_initial.safe_add(self.margin_ratio_maintenance)? / 2
			// }
			MarginRequirementType::Maintenance => self.margin_ratio_maintenance,
		};

		let size_adj_margin_ratio = calculate_size_premium_liability_weight(
			size,
			self.imf_factor,
			default_margin_ratio,
			MARGIN_PRECISION_U128
		)?;

		let margin_ratio = default_margin_ratio.max(size_adj_margin_ratio);

		Ok(margin_ratio)
	}

	pub fn get_max_liquidation_fee(&self) -> NormalResult<u32> {
		let max_liquidation_fee = self.liquidator_fee
			.safe_mul(MAX_LIQUIDATION_MULTIPLIER)?
			.min(
				self.margin_ratio_maintenance
					.safe_mul(LIQUIDATION_FEE_PRECISION)?
					.safe_div(MARGIN_PRECISION)?
			);
		Ok(max_liquidation_fee)
	}

	pub fn update_liquidation_penalty(&mut self, liquidation_penalty: u64) {
		if liquidation_penalty > MAX_PROTOCOL_FEE_RATE {
			return Err(ErrorCode::ProtocolFeeRateMaxExceeded.into());
		}
		self.liquidation_penalty = liquidation_penalty;
	}
}
