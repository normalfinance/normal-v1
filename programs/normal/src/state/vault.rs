use crate::{
	errors::ErrorCode,
	math::{
		tick_index_from_sqrt_price,
		MAX_FEE_RATE,
		MAX_PROTOCOL_FEE_RATE,
		MAX_SQRT_PRICE_X64,
		MIN_SQRT_PRICE_X64,
	},
};
use anchor_lang::prelude::*;

use super::oracle::{ HistoricalOracleData, OracleSource };

#[derive(Clone, Copy, BorshSerialize, BorshDeserialize, PartialEq, Debug, Eq)]
pub enum VaultStatus {
	// Active = 0
	BeingLiquidated = 0b00000001,
	Bankrupt = 0b00000010,
	ReduceOnly = 0b00000100,
}

#[account]
#[derive(Default)]
pub struct Vault {
	/// The address of the vault. It is a pda of the market index
	pub pubkey: Pubkey,
	/// The owner/authority of the account
	pub authority: Pubkey,
	/// An addresses that can control the account on the authority's behalf. Has limited power, cant withdraw
	pub delegate: Pubkey,
	/// The global Vault Config account
	pub config: Pubkey,
	/// The vault used to store the vault's deposits (collateral)
	/// The amount in the vault should be equal to or greater than deposits - liquidity_balance
	pub vault: Pubkey,
	/// Whether the vault is active, being liquidated or bankrupt
	pub status: u8,
	/// The last slot a vault was active. Used to determine if a vault is idle
	pub last_active_slot: u64,
	/// Vault is idle if it's balance has been zero for at least 1 week
	/// Off-chain keeper bots can ignore vaults that are idle
	pub idle: bool,
	/// The total balance lent to the AMM for liquidity
	pub liquidity_balance: u64,
	/// the ratio of collateral value to debt value, which must remain above the liquidation ratio.
	pub collateralization_ratio: u64,
	/// the debt created by minting synthetic against the collateral.
	pub synthetic_tokens_minted: u64,

	pub token_program: u8,
	pub padding: [u8; 12],
}

impl Vault {
	pub fn is_being_liquidated(&self) -> bool {
		self.status &
			((VaultStatus::BeingLiquidated as u8) | (VaultStatus::Bankrupt as u8)) > 0
	}

	pub fn is_bankrupt(&self) -> bool {
		self.status & (VaultStatus::Bankrupt as u8) > 0
	}

	pub fn is_reduce_only(&self) -> bool {
		self.status & (VaultStatus::ReduceOnly as u8) > 0
	}

	pub fn add_user_status(&mut self, status: VaultStatus) {
		self.status |= status as u8;
	}

	pub fn remove_user_status(&mut self, status: VaultStatus) {
		self.status &= !(status as u8);
	}

	pub fn enter_liquidation(&mut self, slot: u64) -> DriftResult<u16> {
		if self.is_being_liquidated() {
			return self.next_liquidation_id.safe_sub(1);
		}

		self.add_user_status(VaultStatus::BeingLiquidated);
		self.liquidation_margin_freed = 0;
		self.last_active_slot = slot;
		Ok(get_then_update_id!(self, next_liquidation_id))
	}

	pub fn exit_liquidation(&mut self) {
		self.remove_user_status(VaultStatus::BeingLiquidated);
		self.remove_user_status(VaultStatus::Bankrupt);
		self.liquidation_margin_freed = 0;
	}

	pub fn enter_bankruptcy(&mut self) {
		self.remove_user_status(VaultStatus::BeingLiquidated);
		self.add_user_status(VaultStatus::Bankrupt);
	}

	pub fn exit_bankruptcy(&mut self) {
		self.remove_user_status(VaultStatus::BeingLiquidated);
		self.remove_user_status(VaultStatus::Bankrupt);
		self.liquidation_margin_freed = 0;
	}

	pub fn update_last_active_slot(&mut self, slot: u64) {
		if !self.is_being_liquidated() {
			self.last_active_slot = slot;
		}
		self.idle = false;
	}

	pub fn update_reduce_only_status(
		&mut self,
		reduce_only: bool
	) -> DriftResult {
		if reduce_only {
			self.add_user_status(VaultStatus::ReduceOnly);
		} else {
			self.remove_user_status(VaultStatus::ReduceOnly);
		}

		Ok(())
	}
}
