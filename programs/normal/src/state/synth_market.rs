use anchor_lang::prelude::*;

use crate::{
	errors::ErrorCode,
	math::{ margin::MarginRequirementType, MAX_PROTOCOL_FEE_RATE },
};

use super::{ amm::AMM, oracle::OracleSource };

use static_assertions::const_assert_eq;

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
pub enum SynthMarketStatus {
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
pub enum SynthTier {
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

impl SynthTier {
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

#[derive(Default, Eq, PartialEq, Debug)]
#[repr(C)]
pub struct InsuranceClaim {
	/// The amount of revenue last settled
	/// Positive if funds left the perp market,
	/// negative if funds were pulled into the perp market
	/// precision: QUOTE_PRECISION
	pub rev_withdraw_since_last_settle: i64,
	/// The max amount of revenue that can be withdrawn per period
	/// precision: QUOTE_PRECISION
	pub max_rev_withdraw_per_period: u64,
	/// The max amount of insurance that perp market can use to resolve bankruptcy and pnl deficits
	/// precision: QUOTE_PRECISION
	pub quote_max_insurance: u64,
	/// The amount of insurance that has been used to resolve bankruptcy and pnl deficits
	/// precision: QUOTE_PRECISION
	pub quote_settled_insurance: u64,
	/// The last time revenue was settled in/out of market
	pub last_revenue_withdraw_ts: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Collateral {
	// pub symbol: Symbol,
	pub mint: Pubkey,
	pub vault: Pubkey,
	pub oracle: Address,
	/// the oracle provider information. used to decode/scale the oracle data
	pub oracle_source: OracleSource,
	pub oracle_frozen: bool,
	/// The sum of the balances for collateral deposits across users
	/// precision: SPOT_BALANCE_PRECISION
	pub balance: u128,
	/// The amount of collateral sent/received with the Pool to adjust price
	pub pool_delta_balance: i128,
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
	// pub auction_config: Auction,
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Synthetic {
	// pub symbol: Symbol,
	pub mint: Pubkey,
	pub vault: Pubkey,
	/// The synthetic tier determines how much insurance a market can receive, with more speculative markets receiving less insurance
	/// It also influences the order markets can be liquidated, with less speculative markets being liquidated first
	pub tier: SynthTier,
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
#[derive(Eq, PartialEq, Debug)]
#[repr(C)]
pub struct SynthMarket {
	/// The market's address. It is a pda of the market index
	pub pubkey: Pubkey,
	pub market_index: u16,
	/// Encoded display name for the market e.g. BTC-SOL
	pub name: [u8; 32],
	pub collateral: Collateral,
	pub synthetic: Synthetic,
	/// The market's token decimals. To from decimals to a precision, 10^decimals
	pub decimals: u32,
	/// Whether a market is active, reduce only, expired, etc
	/// Affects whether users can open/close positions
	pub status: SynthMarketStatus,
	pub paused_operations: u8,
	/// 24hr average of utilization
	/// which is debt amount over collateral amount
	/// precision: SPOT_UTILIZATION_PRECISION
	pub utilization_twap: u64,
	/// Last time the deposit/borrow/utilization averages were updated
	pub last_twap_ts: u64,
	/// The optimatal AMM position to deposit new liquidity into
	pub lp_ts: u64,
	pub last_lp_rebalance_ts: u64,
	/// The ts when the market will be expired. Only set if market is in reduce only mode
	pub expiry_ts: u64,
	/// The price at which positions will be settled. Only set if market is expired
	/// precision = PRICE_PRECISION
	pub expiry_price: i64,
	/// Every deposit has a deposit record id. This is the next id to use
	pub next_deposit_record_id: u64,
	/// The next liquidation id to be used for user
	pub next_liquidation_id: u32,
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
	/// maximum amount of synthetic tokens that can be minted against the market's collateral
	pub debt_ceiling: u128,
	/// minimum amount of synthetic tokens that can be minted against a user's collateral to avoid inefficiencies
	pub debt_floor: u32,
	/// The market's claim on the insurance fund
	pub insurance_claim: InsuranceClaim,
	// Unbacked synthetic tokens (result of collateral auction deficits)
	pub protocol_debt: u64,
	pub padding: [u8; 43],
}

impl Size for SynthMarket {
	const SIZE: usize = 1216; // TODO:
}

impl SynthMarket {
	pub fn is_operation_paused(&self, operation: SynthOperation) -> bool {
		SynthOperation::is_operation_paused(self.paused_operations, operation)
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
		// if liquidation_penalty > MAX_PROTOCOL_FEE_RATE {
		// 	return Err(ErrorCode::ProtocolFeeRateMaxExceeded.into());
		// }
		self.liquidation_penalty = liquidation_penalty;
	}
}
