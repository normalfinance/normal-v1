use anchor_lang::prelude::*;
use enumflags2::BitFlags;

use crate::error::NormalResult;
use crate::constants::constants::{
	FEE_DENOMINATOR,
	FEE_PERCENTAGE_DENOMINATOR,
	MAX_REFERRER_REWARD_EPOCH_UPPER_BOUND,
};
use crate::math::amm::MAX_PROTOCOL_FEE_RATE;
use crate::math::safe_math::SafeMath;
use crate::math::safe_unwrap::SafeUnwrap;
use crate::state::traits::Size;
use crate::{ LAMPORTS_PER_SOL_U64, PERCENTAGE_PRECISION_U64 };

// #[cfg(test)]
// mod tests;

#[account]
#[derive(Default)]
#[repr(C)]
pub struct State {
	pub admin: Pubkey,
	pub signer: Pubkey,
	// rules for validating oracle price data
	pub oracle_guard_rails: OracleGuardRails,
	// prevents transaction from being reused or replayed
	pub signer_nonce: u8,
	pub min_collateral_auction_duration: u8,
	pub default_auction_duration: u8,
	pub exchange_status: u8,
	// account able to update and collect protocol fees
	// pub fee_authority: Pubkey,
	// // account with permissions to collect protocol pool fees
	// pub collect_protocol_fees_authority: Pubkey,
	// // account permissioned to manage pool rewards and emissions
	// pub reward_emissions_super_authority: Pubkey,
	// // the fallback protocol fee for pool swaps
	// pub default_protocol_fee_rate: u16,
	pub padding: [u8; 10],
}

#[derive(BitFlags, Clone, Copy, PartialEq, Debug, Eq)]
pub enum ExchangeStatus {
	// Active = 0b00000000
	DepositPaused = 0b00000001,
	WithdrawPaused = 0b00000010,
	LiqPaused = 0b00000100,
	// Paused = 0b11111111
}

impl ExchangeStatus {
	pub fn active() -> u8 {
		BitFlags::<ExchangeStatus>::empty().bits() as u8
	}
}

impl State {
	pub fn get_exchange_status(&self) -> DriftResult<BitFlags<ExchangeStatus>> {
		BitFlags::<ExchangeStatus>
			::from_bits(usize::from(self.exchange_status))
			.safe_unwrap()
	}

	pub fn amm_paused(&self) -> DriftResult<bool> {
		Ok(self.get_exchange_status()?.contains(ExchangeStatus::AmmPaused))
	}

	pub fn update_fee_authority(&mut self, fee_authority: Pubkey) {
		self.fee_authority = fee_authority;
	}

	pub fn update_collect_protocol_fees_authority(
		&mut self,
		collect_protocol_fees_authority: Pubkey
	) {
		self.collect_protocol_fees_authority = collect_protocol_fees_authority;
	}

	pub fn update_reward_emissions_super_authority(
		&mut self,
		reward_emissions_super_authority: Pubkey
	) {
		self.reward_emissions_super_authority = reward_emissions_super_authority;
	}

	pub fn update_default_protocol_fee_rate(
		&mut self,
		default_protocol_fee_rate: u16
	) -> Result<()> {
		if default_protocol_fee_rate > MAX_PROTOCOL_FEE_RATE {
			return Err(ErrorCode::ProtocolFeeRateMaxExceeded.into());
		}
		self.default_protocol_fee_rate = default_protocol_fee_rate;

		Ok(())
	}
}

impl Size for State {
	const SIZE: usize = 992;
}

#[derive(Copy, AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct OracleGuardRails {
	pub price_divergence: PriceDivergenceGuardRails,
	pub validity: ValidityGuardRails,
}

impl Default for OracleGuardRails {
	fn default() -> Self {
		OracleGuardRails {
			price_divergence: PriceDivergenceGuardRails::default(),
			validity: ValidityGuardRails {
				slots_before_stale_for_amm: 10, // ~5 seconds
				confidence_interval_max_size: 20_000, // 2% of price
				too_volatile_ratio: 5, // 5x or 80% down
			},
		}
	}
}

impl OracleGuardRails {
	pub fn max_oracle_twap_5min_percent_divergence(&self) -> u64 {
		self.price_divergence.oracle_twap_5min_percent_divergence.max(
			PERCENTAGE_PRECISION_U64 / 2
		)
	}
}

#[derive(Copy, AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct PriceDivergenceGuardRails {
	pub mark_oracle_percent_divergence: u64,
	pub oracle_twap_5min_percent_divergence: u64,
}

impl Default for PriceDivergenceGuardRails {
	fn default() -> Self {
		PriceDivergenceGuardRails {
			mark_oracle_percent_divergence: PERCENTAGE_PRECISION_U64 / 10,
			oracle_twap_5min_percent_divergence: PERCENTAGE_PRECISION_U64 / 2,
		}
	}
}

#[derive(Copy, AnchorSerialize, AnchorDeserialize, Clone, Default, Debug)]
pub struct ValidityGuardRails {
	pub slots_before_stale_for_amm: i64,
	pub confidence_interval_max_size: u64,
	pub too_volatile_ratio: i64,
}
