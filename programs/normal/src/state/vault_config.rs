use anchor_lang::prelude::*;

use crate::{ errors::ErrorCode, math::MAX_PROTOCOL_FEE_RATE };

use super::insurance_fund::InsuranceFund;

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Default, Copy)]
pub struct CollateralType {
	identifier: [u8; 32], // Unique identifier (e.g., hash of collateral name)
	mint: Pubkey, // Token mint address for collateral type
	liquidation_ratio: u64, // Minimum collateralization ratio - the threshold at which the vault becomes eligible for liquidation (usually the same as the minimum collateralization ratio).
	liquidation_penalty: u64, /// A fee applied to the collateral when the vault is liquidated, incentivizing users to maintain sufficient collateral.
}

#[account]
pub struct VaultsConfig {
	/// The token mint of the vaults
	pub mint: Pubkey,
	/// oracle price data public key
	pub oracle: Pubkey,
	/// The AMM
	pub amm: Pubkey,

	// reference to the collateral type identifier
	pub collateral_type: [u8; 32],

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
}

impl VaultsConfig {
	pub const LEN: usize = 8 + 96 + 4;

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
