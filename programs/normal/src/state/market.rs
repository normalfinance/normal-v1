use anchor_lang::prelude::*;

use crate::{ errors::ErrorCode, math::MAX_PROTOCOL_FEE_RATE };

use super::{
	collateral::Collateral,
	insurance::{ InsuranceClaim, InsuranceFund },
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
	/// warm up period for initialization, fills are paused
	#[default]
	Initialized,
	/// all operations allowed
	Active,
	/// Deprecated in favor of PausedOperations
	FundingPaused,
	/// Deprecated in favor of PausedOperations
	AmmPaused,
	/// Deprecated in favor of PausedOperations
	FillPaused,
	/// Deprecated in favor of PausedOperations
	WithdrawPaused,
	/// fills only able to reduce liability
	ReduceOnly,
	/// market has determined settlement price and positions are expired must be settled
	Settlement,
	/// market has no remaining participants
	Delisted,
}

impl MarketStatus {
	pub fn validate_not_deprecated(&self) -> NormalResult {
		if
			matches!(
				self,
				MarketStatus::FundingPaused |
					MarketStatus::AmmPaused |
					MarketStatus::FillPaused |
					MarketStatus::WithdrawPaused
			)
		{
			msg!("MarketStatus is deprecated");
			Err(ErrorCode::DefaultError)
		} else {
			Ok(())
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
pub enum SyntheticType {
	#[default]
	Asset,
	IndexFund,
	Yield,
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

#[account]
pub struct Market {
	/// The market's address. It is a pda of the market index
	pub pubkey: Pubkey,
	/// oracle price data public key
	pub oracle: Pubkey,
	/// The AMM
	pub amm: Pubkey,
	/// Encoded display name for the market e.g. SOL-PERP
	pub name: [u8; 32],
	/// sdfd
	pub collateral: Collateral,
	/// The perp market's claim on the insurance fund
	pub insurance_claim: InsuranceClaim,

	/// The token mint of the vaults
	// pub mint: Pubkey,

	/// number of users in a position (pnl) or pnl (quote)
	pub number_of_users: u32,
	pub market_index: u16,
	/// Whether a market is active, reduce only, expired, etc
	/// Affects whether users can open/close positions
	pub status: MarketStatus,
	/// Currently only Perpetual markets are supported
	pub synthetic_type: SyntheticType,
	/// The contract tier determines how much insurance a market can receive, with more speculative markets receiving less insurance
	/// It also influences the order perp markets can be liquidated, with less speculative markets being liquidated first
	pub synthetic_tier: SyntheticTier,
	pub paused_operations: u8,

	///
	pub min_collateral_ratio: u64,
	/// maximum amount of synthetic that can be generated against a specific collateral type, ensuring that the system does not become overexposed to any one asset.
	pub debt_ceiling: u64,

	pub default_liquidity_utilization: u64,
	/// The maximum percent of the collateral that can be sent to the AMM as liquidity
	pub max_liquidity_utilization: u64,

	/// Auction Config
	///
	/// where collateral auctions should take place (3rd party AMM vs private)
	pub auction_preference: AuctionPreference,
	/// Initial auction price for the collateral, typically set above market value to incentivize participation.
	pub start_price: u64,
	/// Maximum time allowed for the auction to complete.
	pub duration: u16,
	/// Determines how quickly the starting price decreases during the auction if there are no bids.
	pub bid_decrease_rate: u16,
	/// The amount of collateral being auctioned.
	pub lot_size: u64,
	/// May be capped to prevent overly large auctions that could affect the market price.
	pub max_lot_size: u64,

	pub padding: [u8; 43],
}

impl Default for Market {
	fn default() -> Self {
		Market {
			pubkey: Pubkey::default(),
			amm: Pubkey::default(),

			collateral: Collateral::default(),

			padding: [0; 43],
		}
	}
}

impl Size for Market {
	const SIZE: usize = 1216;
}

impl Market {
	pub fn is_operation_paused(&self, operation: SynthOperation) -> bool {
        SynthOperation::is_operation_paused(self.paused_operations, operation)
    }

	pub fn get_margin_ratio(
        &self,
        size: u128,
        margin_type: MarginRequirementType,
    ) -> NormalResult<u32> {
        if self.status == MarketStatus::Settlement {
            return Ok(0); // no liability weight on size
        }

        let default_margin_ratio = match margin_type {
            MarginRequirementType::Initial => self.margin_ratio_initial,
            MarginRequirementType::Fill => {
                self.margin_ratio_initial
                    .safe_add(self.margin_ratio_maintenance)?
                    / 2
            }
            MarginRequirementType::Maintenance => self.margin_ratio_maintenance,
        };

        let size_adj_margin_ratio = calculate_size_premium_liability_weight(
            size,
            self.imf_factor,
            default_margin_ratio,
            MARGIN_PRECISION_U128,
        )?;

        let margin_ratio = default_margin_ratio.max(size_adj_margin_ratio);

        Ok(margin_ratio)
    }

    pub fn get_max_liquidation_fee(&self) -> NormalResult<u32> {
        let max_liquidation_fee = (self.liquidator_fee.safe_mul(MAX_LIQUIDATION_MULTIPLIER)?).min(
            self.margin_ratio_maintenance
                .safe_mul(LIQUIDATION_FEE_PRECISION)?
                .safe_div(MARGIN_PRECISION)?,
        );
        Ok(max_liquidation_fee)
    }

	pub fn initialize(
		&mut self,
		reward_emissions_super_authority: Pubkey,
		default_protocol_fee_rate: u16
	) -> Result<()> {
		self.fee_authority = fee_authority;
		self.collect_protocol_fees_authority = collect_protocol_fees_authority;
		self.reward_emissions_super_authority = reward_emissions_super_authority;
		self.update_default_protocol_fee_rate(default_protocol_fee_rate)?;

		Ok(())
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
