use anchor_lang::prelude::*;
use enumflags2::BitFlags;

use crate::error::NormalResult;
use crate::constants::main::{
	FEE_DENOMINATOR,
	FEE_PERCENTAGE_DENOMINATOR,
	MAX_REFERRER_REWARD_EPOCH_UPPER_BOUND,
};
use crate::math::amm::MAX_PROTOCOL_FEE_RATE;
use crate::math::safe_math::SafeMath;
use crate::math::safe_unwrap::SafeUnwrap;
use crate::state::traits::Size;
use crate::{ LAMPORTS_PER_SOL_U64, PERCENTAGE_PRECISION_U64 };

use super::synth_market::AuctionConfig;

#[derive(
	Default,
	Clone,
	Copy,
	BorshSerialize,
	BorshDeserialize,
	PartialEq,
	Debug,
	Eq
)]
pub enum MarketType {
	#[default]
	Synth,
	Index,
}

impl fmt::Display for MarketType {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			MarketType::Synth => write!(f, "Synth"),
			MarketType::Index => write!(f, "Index"),
		}
	}
}

#[account]
#[derive(Default)]
#[repr(C)]
pub struct State {
	pub admin: Pubkey,
	pub signer: Pubkey,
	// ensures signer transaction are not duplicated
	pub signer_nonce: u8,

	// Oracle
	//
	// validations to ensure oracle prices are accurate and reliable
	pub oracle_guard_rails: OracleGuardRails,
	// set of elected keepers who can freeze/update oracles in an emergency
	pub emergency_oracles: Vec<>,

	// Exchange/AMMs
	//
	// the current status of the protocol
	pub exchange_status: u8,
	// the total number of markets live on the protocol
	pub number_of_markets: u16,
	// the total number of index markets live on the protocol
	pub number_of_index_markets: u16,

	// Index
	//
	pub default_index_oracle: Pubkey,
	pub max_index_assets: u16,
	pub protocol_index_fee: u16,
	pub protocol_index_fee_vault: Pubkey,

	// Insurance Fund
	//
	pub insurance_fund: Pubkey,

	pub total_debt_ceiling: u64,

	// User
	//
	// ensures user inititialization does not become costly
	pub max_initialize_user_fee: u16,
	// tracks the number of User delegate authorities
	pub number_of_authorities: u64,
	// tracks the number of User sub-accounts used to partition Vaults
	pub number_of_sub_accounts: u64,
	// the maximum number of sub-accounts the protocol is willing to support
	pub max_number_of_sub_accounts: u16,

	// Liquidation
	//

	/// The maximum percent of the collateral that can be sent to the AMM as liquidity
	// pub max_amm_liquidity_utilization: u64,
	pub liquidation_margin_buffer_ratio: u32,
	pub liquidation_duration: u8,
	pub initial_pct_to_liquidate: u16,

	// Debt Auctions
	pub debt_auction_config: AuctionConfig,

	pub dca_order_padding: u16,

	pub padding: [u8; 10],
}

#[derive(BitFlags, Clone, Copy, PartialEq, Debug, Eq)]
pub enum ExchangeStatus {
	// Active = 0b00000000
	DepositPaused = 0b00000001,
	WithdrawPaused = 0b00000010,
	LendPaused = 0b00000100,
	AmmPaused = 0b00001000,
	LiqPaused = 0b00010000,
	ScheduleFillPaused = 0b00100000,
	// Paused = 0b11111111
}

impl ExchangeStatus {
	pub fn active() -> u8 {
		BitFlags::<ExchangeStatus>::empty().bits() as u8
	}
}

impl State {
	pub fn get_exchange_status(&self) -> NormalResult<BitFlags<ExchangeStatus>> {
		BitFlags::<ExchangeStatus>
			::from_bits(usize::from(self.exchange_status))
			.safe_unwrap()
	}

	pub fn max_number_of_sub_accounts(&self) -> u64 {
		if self.max_number_of_sub_accounts <= 5 {
			return self.max_number_of_sub_accounts as u64;
		}

		(self.max_number_of_sub_accounts as u64).saturating_mul(100)
	}

	pub fn get_init_user_fee(&self) -> NormalResult<u64> {
		let max_init_fee: u64 =
			((self.max_initialize_user_fee as u64) * LAMPORTS_PER_SOL_U64) / 100;

		let target_utilization: u64 = (8 * PERCENTAGE_PRECISION_U64) / 10;

		let account_space_utilization: u64 = self.number_of_sub_accounts
			.safe_mul(PERCENTAGE_PRECISION_U64)?
			.safe_div(self.max_number_of_sub_accounts().max(1))?;

		let init_fee: u64 = if account_space_utilization > target_utilization {
			max_init_fee
				.safe_mul(account_space_utilization.safe_sub(target_utilization)?)?
				.safe_div(PERCENTAGE_PRECISION_U64.safe_sub(target_utilization)?)?
		} else {
			0
		};

		Ok(init_fee)
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
