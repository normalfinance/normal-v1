use anchor_lang::prelude::*;

use crate::{ errors::ErrorCode, math::MAX_PROTOCOL_FEE_RATE };

#[assert_no_slop]
#[zero_copy(unsafe)]
#[derive(Debug, PartialEq, Eq)]
#[repr(C)]
pub struct Collateral {
	pub mint: Pubkey, // Token mint address for collateral type
	pub identifier: [u8; 32], // Unique identifier (e.g., hash of collateral name)

	pub liquidation_ratio: u64, // Minimum collateralization ratio - the threshold at which the vault becomes eligible for liquidation (usually the same as the minimum collateralization ratio).
	pub liquidation_penalty: u64, // A fee applied to the collateral when the vault is liquidated, incentivizing users to maintain sufficient collateral.
}
