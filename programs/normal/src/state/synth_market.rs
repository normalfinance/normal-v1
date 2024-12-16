use anchor_lang::prelude::*;

use crate::{
	errors::ErrorCode,
	math::{ margin::MarginRequirementType, MAX_PROTOCOL_FEE_RATE },
};

use super::{
	amm::AMM,
	collateral::Collateral,
	insurance::{ InsuranceClaim, InsuranceFund },
	oracle::OracleSource,
};

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
pub enum SyntheticTier {
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

impl SyntheticTier {
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
pub enum AuctionType {
	#[default]
	/// selling collateral from a Vault liquidation
	Collateral,
	/// selling newly minted NORM to cover Protocol Debt (the deficit from Collateral Auctions)
	Debt,
	/// selling excess synthetic token proceeds over the Insurance Fund max limit for NORM to be burned
	Surplus,
}

#[derive(Copy, AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct AuctionConfig {
	/// where collateral auctions should take place (3rd party AMM vs private)
	pub auction_location: AuctionPreference,
	/// Maximum time allowed for the auction to complete.
	pub auction_duration: u16,
	/// Determines how quickly the starting price decreases during the auction if there are no bids.
	pub auction_bid_decrease_rate: u16,
	/// May be capped to prevent overly large auctions that could affect the market price.
	pub max_auction_lot_size: u64,
}

#[account]
pub struct SynthMarket {
	/// The market's address. It is a pda of the market index
	pub pubkey: Pubkey,
	pub market_index: u16,
	/// Encoded display name for the market e.g. BTC-SOL
	pub name: [u8; 32],
	/// Whether a market is active, reduce only, expired, etc
	/// Affects whether users can open/close positions
	pub status: MarketStatus,
	/// The contract tier determines how much insurance a market can receive, with more speculative markets receiving less insurance
	/// It also influences the order markets can be liquidated, with less speculative markets being liquidated first
	pub synthetic_tier: SyntheticTier,
	pub paused_operations: u8,
	pub number_of_users: u32,

	// From Vault
	// Metrics
	//

	/// The total balance lent to 3rd party protocols
	pub collateral_loan_balance: u64,

	/// The vault used to store the vault's deposits (collateral)
	/// The amount in the vault should be equal to or greater than deposits - loan_balance
	pub token_vault_collateral: Pubkey,

	/// Whether the vault is active, being liquidated or bankrupt
	pub status: u8,
	/// The last slot a vault was active. Used to determine if a vault is idle
	pub last_active_slot: u64,
	/// Vault is idle if it's balance has been zero for at least 1 week
	/// Off-chain keeper bots can ignore vaults that are idle
	pub idle: bool,

	/// the ratio of collateral value to debt value, which must remain above the liquidation ratio.
	pub collateralization_ratio: u64,
	/// the debt created by minting synthetic against the collateral.
	pub synthetic_tokens_minted: u64,



	// Oracle
	//
	/// oracle price data public key
	pub oracle: Pubkey,
	/// the oracle provider information. used to decode/scale the oracle public key
	pub oracle_source: OracleSource,
	/// stores historically witnessed oracle data
	pub historical_oracle_data: HistoricalOracleData,
	/// the pct size of the oracle confidence interval
	/// precision: PERCENTAGE_PRECISION
	pub last_oracle_conf_pct: u64,
	/// tracks whether the oracle was considered valid at the last AMM update
	pub last_oracle_valid: bool,
	/// the last seen oracle price partially shrunk toward the amm reserve price
	/// precision: PRICE_PRECISION
	pub last_oracle_normalised_price: i64,
	/// the gap between the oracle price and the reserve price = y * peg_multiplier / x
	pub last_oracle_reserve_price_spread_pct: i64,
	/// estimate of standard deviation of the oracle price at each update
	/// precision: PRICE_PRECISION
	pub oracle_std: u64,

	// Collateral / Liquidations
	//
	// Mint for the collateral token
	pub token_mint_collateral: Pubkey,
	// Vault storing synthetic tokens from liquidation
	pub token_vault_synthetic: Pubkey,
	// Vault storing collateral tokens for auction
	pub token_vault_collateral: Pubkey,
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
	/// The initial margin fraction factor. Used to increase margin ratio for large positions
	/// precision: MARGIN_PRECISION
	pub imf_factor: u32,
	/// maximum amount of synthetic tokens that can be minted against the market's collateral
	pub debt_ceiling: u128,
	/// minimum amount of synthetic tokens that can be minted against a user's collateral to avoid inefficiencies
	pub debt_floor: u32,
	///
	pub collateral_lending_utilization: u64,

	// AMM
	//
	pub amm: Pubkey,

	// Insurance
	//
	/// The market's claim on the insurance fund
	pub insurance_claim: InsuranceClaim,
	/// The total socialized loss from borrows, in the mint's token
	/// precision: token mint precision
	pub total_gov_token_inflation: u128,

	/// Auction Config
	///
	/// where collateral auctions should take place (3rd party AMM vs private)
	pub collateral_action_config: AuctionConfig,

	// Metrics
	//
	// Total synthetic token debt
	pub outstanding_debt: u128,
	// Unbacked synthetic tokens (result of collateral auction deficits)
	pub protocol_debt: u64,

	// Shutdown
	//
	/// The ts when the market will be expired. Only set if market is in reduce only mode
	pub expiry_ts: i64,
	/// The price at which positions will be settled. Only set if market is expired
	/// precision = PRICE_PRECISION
	pub expiry_price: i64,

	pub padding: [u8; 43],
}

impl Default for SynthMarket {
	fn default() -> Self {
		SynthMarket {
			pubkey: Pubkey::default(),
			market_index: 0,
			name: [0; 32],
			status: MarketStatus::default(),
			synthetic_tier: SyntheticTier::default(),
			paused_operations: 0,
			number_of_users: 0,

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

				protocol_fee_owed_synthetic: 0,
				protocol_fee_owed_quote: 0,

				token_mint_synthetic,
				token_vault_synthetic,
				fee_growth_global_synthetic: 0,

				token_mint_quote,
				token_vault_quote,
				fee_growth_global_quote: 0,

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

impl Size for SynthMarket {
	const SIZE: usize = 1216; // TODO:
}

impl SynthMarket {
	pub fn is_operation_paused(&self, operation: SynthOperation) -> bool {
		SynthOperation::is_operation_paused(self.paused_operations, operation)
	}

	pub fn update_debt_ceiling(&self, debt_ceiling: u64) -> NormalResult<u64> {
		if debt_ceiling > MAX_PROTOCOL_FEE_RATE {
			return Err(ErrorCode::ProtocolFeeRateMaxExceeded.into());
		}

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

#[derive(
	Clone,
	Copy,
	AnchorSerialize,
	AnchorDeserialize,
	PartialEq,
	Debug,
	Eq,
	Default
)]
pub enum AuctionPreference {
	#[default]
	/// a local secondary market
	Private,
	/// a DEX like Orca, Serum, Jupiter, etc.
	External,
}
