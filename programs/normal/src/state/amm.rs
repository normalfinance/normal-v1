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

#[account]
#[derive(Default)]
pub struct AMM {
	pub amm_bump: [u8; 1],

	/// the pubkey of the collateral Vault backing the value of the pool’s synthetic asset
	pub vault: Pubkey,
	/// the authority that can push or pull quote asset tokens to/from the Vault when price exceed the max_price_deviance
	pub vault_balance_authority: Pubkey,

	/// Tokens
	///
	/// Mint for the synthetic token
	pub token_mint_synthetic: Pubkey,
	/// Mint for the quote token (SOL, XLM, USDC)
	pub token_mint_quote: Pubkey,

	/// Vault storing synthetic tokens
	pub token_vault_synthetic: Pubkey,
	/// Vault storing quote tokens (SOL, XLM, USDC)
	pub token_vault_quote: Pubkey,

	pub risk_tier: SyntheticTier,

	/// Oracle
	///
	/// oracle price data public key
	pub oracle: Pubkey,
	/// the oracle provider information. used to decode/scale the oracle public key
	pub oracle_source: OracleSource,
	/// stores historically witnessed oracle data
	pub historical_oracle_data: HistoricalOracleData,
	pub historical_index_data: HistoricalIndexData,
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

	/// Peg
	///
	/// the maximum percent the pool price can deviate above or below the oracle twap
	pub max_price_deviance: u16,
	/// volume divided by synthetic token market cap (how much volume is created per $1 of liquidity)
	pub liquidity_to_volume_multiplier: u64,

	/// Liquidity
	///
	pub tick_spacing: u16,
	pub tick_spacing_seed: [u8; 2],
	pub tick_current_index: i32,
	// Maximum amount that can be held by Solana account
	pub liquidity: u128,
	// MAX/MIN at Q32.64, but using Q64.64 for rounder bytes
	// Q64.64
	pub sqrt_price: u128,

	/// Fees
	///
	// Stored as hundredths of a basis point
	// u16::MAX corresponds to ~6.5%
	pub fee_rate: u16,
	// Portion of fee rate taken stored as basis points
	pub protocol_fee_rate: u16,
	/// portion of the fee rate sent to the Insurance Fund as basis points
	pub insurance_fund_fee_rate: u16,

	pub fee_growth_global_synthetic: u128,
	pub fee_growth_global_quote: u128,

	pub protocol_fee_owed_synthetic: u64,
	pub protocol_fee_owed_quote: u64,

	/// Rewards
	///
	pub reward_last_updated_timestamp: u64,
	pub reward_infos: [AMMRewardInfo; NUM_REWARDS], // 384
}

// Number of rewards supported by AMMs
pub const NUM_REWARDS: usize = 3;

impl AMM {
	pub const LEN: usize = 8 + 261 + 384;

	pub fn is_price_inside_range(&self, price: u64) -> bool {
		if price < 0 {
			0
		} else if swap_update.next_sqrt_price < limit {
			1
		} else {
			true
		}
	}

	pub fn seeds(&self) -> [&[u8]; 6] {
		[
			&b"amm"[..],
			self.token_mint_synthetic.as_ref(),
			self.token_mint_quote.as_ref(),
			self.tick_spacing_seed.as_ref(),
			self.amm_bump.as_ref(),
		]
	}

	pub fn input_token_mint(&self, synthetic_to_quote: bool) -> Pubkey {
		if synthetic_to_quote {
			self.token_mint_synthetic
		} else {
			self.token_mint_quote
		}
	}

	pub fn input_token_vault(&self, synthetic_to_quote: bool) -> Pubkey {
		if synthetic_to_quote {
			self.token_vault_synthetic
		} else {
			self.token_vault_quote
		}
	}

	pub fn output_token_mint(&self, synthetic_to_quote: bool) -> Pubkey {
		if synthetic_to_quote {
			self.token_mint_quote
		} else {
			self.token_mint_synthetic
		}
	}

	pub fn output_token_vault(&self, synthetic_to_quote: bool) -> Pubkey {
		if synthetic_to_quote {
			self.token_vault_quote
		} else {
			self.token_vault_synthetic
		}
	}

	#[allow(clippy::too_many_arguments)]
	pub fn initialize(
		&mut self,
		bump: u8,
		tick_spacing: u16,
		sqrt_price: u128,
		default_fee_rate: u16,
		token_mint_synthetic: Pubkey,
		token_vault_synthetic: Pubkey,
		token_mint_quote: Pubkey,
		token_vault_quote: Pubkey,
		// cusotm
		oracle: Pubkey,
		oracle_source: OracleSource
	) -> Result<()> {
		if token_mint_synthetic.ge(&token_mint_quote) {
			return Err(ErrorCode::InvalidTokenMintOrder.into());
		}

		if !(MIN_SQRT_PRICE_X64..=MAX_SQRT_PRICE_X64).contains(&sqrt_price) {
			return Err(ErrorCode::SqrtPriceOutOfBounds.into());
		}

		self.amm_bump = [bump];

		// Tokens
		self.token_mint_synthetic = token_mint_synthetic;
		self.token_vault_synthetic = token_vault_synthetic;

		self.token_mint_quote = token_mint_quote;
		self.token_vault_quote = token_vault_quote;

		// Oracle
		self.oracle = oracle.key();
		self.oracle_source = oracle_source;
		self.historical_oracle_data = HistoricalOracleData::default();
		self.historical_index_data = HistoricalIndexData::default();
		self.last_oracle_conf_pct = 0;
		self.last_oracle_valid = false;
		self.last_oracle_normalised_price = 0;
		self.last_oracle_reserve_price_spread_pct = 0;
		self.oracle_std = 0;

		// Liquidity
		self.sqrt_price = sqrt_price;
		self.liquidity = 0;
		self.tick_spacing = tick_spacing;
		self.tick_spacing_seed = self.tick_spacing.to_le_bytes();
		self.tick_current_index = tick_index_from_sqrt_price(&sqrt_price);

		// Fees

		self.update_fee_rate(default_fee_rate)?;
		self.update_protocol_fee_rate(amms_config.default_protocol_fee_rate)?;

		self.protocol_fee_owed_synthetic = 0;
		self.protocol_fee_owed_quote = 0;

		self.fee_growth_global_synthetic = 0;
		self.fee_growth_global_quote = 0;

		// Rewards
		self.reward_infos = [
			AMMRewardInfo::new(amms_config.reward_emissions_super_authority);
			NUM_REWARDS
		];

		Ok(())
	}

	/// Update all reward values for the AMM.
	///
	/// # Parameters
	/// - `reward_infos` - An array of all updated amm rewards
	/// - `reward_last_updated_timestamp` - The timestamp when the rewards were last updated
	pub fn update_rewards(
		&mut self,
		reward_infos: [AMMRewardInfo; NUM_REWARDS],
		reward_last_updated_timestamp: u64
	) {
		self.reward_last_updated_timestamp = reward_last_updated_timestamp;
		self.reward_infos = reward_infos;
	}

	pub fn update_rewards_and_liquidity(
		&mut self,
		reward_infos: [AMMRewardInfo; NUM_REWARDS],
		liquidity: u128,
		reward_last_updated_timestamp: u64
	) {
		self.update_rewards(reward_infos, reward_last_updated_timestamp);
		self.liquidity = liquidity;
	}

	/// Update the reward authority at the specified AMM reward index.
	pub fn update_reward_authority(
		&mut self,
		index: usize,
		authority: Pubkey
	) -> Result<()> {
		if index >= NUM_REWARDS {
			return Err(ErrorCode::InvalidRewardIndex.into());
		}
		self.reward_infos[index].authority = authority;

		Ok(())
	}

	pub fn update_emissions(
		&mut self,
		index: usize,
		reward_infos: [AMMRewardInfo; NUM_REWARDS],
		timestamp: u64,
		emissions_per_second_x64: u128
	) -> Result<()> {
		if index >= NUM_REWARDS {
			return Err(ErrorCode::InvalidRewardIndex.into());
		}
		self.update_rewards(reward_infos, timestamp);
		self.reward_infos[index].emissions_per_second_x64 =
			emissions_per_second_x64;

		Ok(())
	}

	pub fn initialize_reward(
		&mut self,
		index: usize,
		mint: Pubkey,
		vault: Pubkey
	) -> Result<()> {
		if index >= NUM_REWARDS {
			return Err(ErrorCode::InvalidRewardIndex.into());
		}

		let lowest_index = match
			self.reward_infos.iter().position(|r| !r.initialized())
		{
			Some(lowest_index) => lowest_index,
			None => {
				return Err(ErrorCode::InvalidRewardIndex.into());
			}
		};

		if lowest_index != index {
			return Err(ErrorCode::InvalidRewardIndex.into());
		}

		self.reward_infos[index].mint = mint;
		self.reward_infos[index].vault = vault;

		Ok(())
	}

	#[allow(clippy::too_many_arguments)]
	pub fn update_after_swap(
		&mut self,
		liquidity: u128,
		tick_index: i32,
		sqrt_price: u128,
		fee_growth_global: u128,
		reward_infos: [AMMRewardInfo; NUM_REWARDS],
		protocol_fee: u64,
		is_token_fee_in_synthetic: bool,
		reward_last_updated_timestamp: u64
	) {
		self.tick_current_index = tick_index;
		self.sqrt_price = sqrt_price;
		self.liquidity = liquidity;
		self.reward_infos = reward_infos;
		self.reward_last_updated_timestamp = reward_last_updated_timestamp;
		if is_token_fee_in_synthetic {
			// Add fees taken via a
			self.fee_growth_global_synthetic = fee_growth_global;
			self.protocol_fee_owed_synthetic += protocol_fee;
		} else {
			// Add fees taken via b
			self.fee_growth_global_quote = fee_growth_global;
			self.protocol_fee_owed_quote += protocol_fee;
		}
	}

	pub fn update_fee_rate(&mut self, fee_rate: u16) -> Result<()> {
		if fee_rate > MAX_FEE_RATE {
			return Err(ErrorCode::FeeRateMaxExceeded.into());
		}
		self.fee_rate = fee_rate;

		Ok(())
	}

	pub fn update_protocol_fee_rate(
		&mut self,
		protocol_fee_rate: u16
	) -> Result<()> {
		if protocol_fee_rate > MAX_PROTOCOL_FEE_RATE {
			return Err(ErrorCode::ProtocolFeeRateMaxExceeded.into());
		}
		self.protocol_fee_rate = protocol_fee_rate;

		Ok(())
	}

	pub fn reset_protocol_fees_owed(&mut self) {
		self.protocol_fee_owed_synthetic = 0;
		self.protocol_fee_owed_quote = 0;
	}

	pub fn get_oracle_twap(
		&self,
		price_oracle: &AccountInfo,
		slot: u64
	) -> NormalResult<Option<i64>> {
		match self.oracle_source {
			OracleSource::Pyth | OracleSource::PythStableCoin => {
				Ok(Some(self.get_pyth_twap(price_oracle, 1, false)?))
			}
			OracleSource::Pyth1K =>
				Ok(Some(self.get_pyth_twap(price_oracle, 1000, false)?)),
			OracleSource::Pyth1M =>
				Ok(Some(self.get_pyth_twap(price_oracle, 1000000, false)?)),
			OracleSource::Switchboard =>
				Ok(Some(get_switchboard_price(price_oracle, slot)?.price)),
			OracleSource::SwitchboardOnDemand => {
				Ok(Some(get_sb_on_demand_price(price_oracle, slot)?.price))
			}
			OracleSource::QuoteAsset => {
				msg!("Can't get oracle twap for quote asset");
				Err(ErrorCode::DefaultError)
			}
			OracleSource::PythPull | OracleSource::PythStableCoinPull => {
				Ok(Some(self.get_pyth_twap(price_oracle, 1, true)?))
			}
			OracleSource::Pyth1KPull =>
				Ok(Some(self.get_pyth_twap(price_oracle, 1000, true)?)),
			OracleSource::Pyth1MPull => {
				Ok(Some(self.get_pyth_twap(price_oracle, 1000000, true)?))
			}
		}
	}

	pub fn get_pyth_twap(
		&self,
		price_oracle: &AccountInfo,
		multiple: u128,
		is_pull_oracle: bool
	) -> NormalResult<i64> {
		let mut pyth_price_data: &[u8] = &price_oracle
			.try_borrow_data()
			.or(Err(ErrorCode::UnableToLoadOracle))?;

		let oracle_price: i64;
		let oracle_twap: i64;
		let oracle_exponent: i32;

		if is_pull_oracle {
			let price_message = pyth_solana_receiver_sdk::price_update::PriceUpdateV2
				::try_deserialize(&mut pyth_price_data)
				.or(Err(crate::error::ErrorCode::UnableToLoadOracle))?;
			oracle_price = price_message.price_message.price;
			oracle_twap = price_message.price_message.ema_price;
			oracle_exponent = price_message.price_message.exponent;
		} else {
			let price_data = pyth_client::cast::<pyth_client::Price>(pyth_price_data);
			oracle_price = price_data.agg.price;
			oracle_twap = price_data.twap.val;
			oracle_exponent = price_data.expo;
		}

		assert!(oracle_twap > oracle_price / 10);

		let oracle_precision = (10_u128)
			.pow(oracle_exponent.unsigned_abs())
			.safe_div(multiple)?;

		let mut oracle_scale_mult = 1;
		let mut oracle_scale_div = 1;

		if oracle_precision > PRICE_PRECISION {
			oracle_scale_div = oracle_precision.safe_div(PRICE_PRECISION)?;
		} else {
			oracle_scale_mult = PRICE_PRECISION.safe_div(oracle_precision)?;
		}

		oracle_twap
			.cast::<i128>()?
			.safe_mul(oracle_scale_mult.cast()?)?
			.safe_div(oracle_scale_div.cast()?)?
			.cast::<i64>()
	}

	pub fn get_new_oracle_conf_pct(
		&self,
		confidence: u64, // price precision
		reserve_price: u64, // price precision
		now: i64
	) -> NormalResult<u64> {
		// use previous value decayed as lower bound to avoid shrinking too quickly
		let upper_bound_divisor = 21_u64;
		let lower_bound_divisor = 5_u64;
		let since_last = now
			.safe_sub(self.historical_oracle_data.last_oracle_price_twap_ts)?
			.max(0);

		let confidence_lower_bound = if since_last > 0 {
			let confidence_divisor = upper_bound_divisor
				.saturating_sub(since_last.cast::<u64>()?)
				.max(lower_bound_divisor);
			self.last_oracle_conf_pct.safe_sub(
				self.last_oracle_conf_pct / confidence_divisor
			)?
		} else {
			self.last_oracle_conf_pct
		};

		Ok(
			confidence
				.safe_mul(BID_ASK_SPREAD_PRECISION)?
				.safe_div(reserve_price)?
				.max(confidence_lower_bound)
		)
	}

	pub fn is_recent_oracle_valid(
		&self,
		current_slot: u64
	) -> NormalResult<bool> {
		Ok(self.last_oracle_valid && current_slot == self.last_update_slot)
	}

	pub fn is_price_divergence_ok(
		&self,
		oracle_price: i64
	) -> NormalResult<bool> {
		let oracle_divergence = oracle_price
			.safe_sub(self.historical_oracle_data.last_oracle_price_twap_5min)?
			.safe_mul(PERCENTAGE_PRECISION_I64)?
			.safe_div(
				self.historical_oracle_data.last_oracle_price_twap_5min.min(
					oracle_price
				)
			)?
			.unsigned_abs();

		let oracle_divergence_limit = match self.synthetic_tier {
			SyntheticTier::A => PERCENTAGE_PRECISION_U64 / 200, // 50 bps
			SyntheticTier::B => PERCENTAGE_PRECISION_U64 / 200, // 50 bps
			SyntheticTier::C => PERCENTAGE_PRECISION_U64 / 100, // 100 bps
			SyntheticTier::Speculative => PERCENTAGE_PRECISION_U64 / 40, // 250 bps
			SyntheticTier::HighlySpeculative => PERCENTAGE_PRECISION_U64 / 40, // 250 bps
			SyntheticTier::Isolated => PERCENTAGE_PRECISION_U64 / 40, // 250 bps
		};

		if oracle_divergence >= oracle_divergence_limit {
			msg!(
				"market_index={} price divergence too large to safely settle pnl: {} >= {}",
				self.market_index,
				oracle_divergence,
				oracle_divergence_limit
			);
			return Ok(false);
		}

		let min_price = oracle_price.min(
			self.historical_oracle_data.last_oracle_price_twap_5min
		);

		let std_limit = (
			match self.synthetic_tier {
				SyntheticTier::A => min_price / 50, // 200 bps
				SyntheticTier::B => min_price / 50, // 200 bps
				SyntheticTier::C => min_price / 20, // 500 bps
				SyntheticTier::Speculative => min_price / 10, // 1000 bps
				SyntheticTier::HighlySpeculative => min_price / 10, // 1000 bps
				SyntheticTier::Isolated => min_price / 10, // 1000 bps
			}
		).unsigned_abs();

		if self.oracle_std.max(self.mark_std) >= std_limit {
			msg!(
				"market_index={} std too large to safely settle pnl: {} >= {}",
				self.market_index,
				self.oracle_std.max(self.mark_std),
				std_limit
			);
			return Ok(false);
		}

		Ok(true)
	}

	pub fn is_index_fund_market(&self) -> bool {
		self.SyntheticType == SyntheticType::IndexFund
	}

	pub fn is_yield_market(&self) -> bool {
		self.SyntheticType == SyntheticType::Yield
	}

	pub fn get_max_confidence_interval_multiplier(self) -> NormalResult<u64> {
		// assuming validity_guard_rails max confidence pct is 2%
		Ok(match self.synthetic_tier {
			SyntheticTier::A => 1, // 2%
			SyntheticTier::B => 1, // 2%
			SyntheticTier::C => 2, // 4%
			SyntheticTier::Speculative => 10, // 20%
			SyntheticTier::HighlySpeculative => 50, // 100%
			SyntheticTier::Isolated => 50, // 100%
		})
	}

	pub fn get_sanitize_clamp_denominator(self) -> NormalResult<Option<i64>> {
		Ok(match self.synthetic_tier {
			SyntheticTier::A => Some(10_i64), // 10%
			SyntheticTier::B => Some(5_i64), // 20%
			SyntheticTier::C => Some(2_i64), // 50%
			SyntheticTier::Speculative => None, // DEFAULT_MAX_TWAP_UPDATE_PRICE_BAND_DENOMINATOR
			SyntheticTier::HighlySpeculative => None, // DEFAULT_MAX_TWAP_UPDATE_PRICE_BAND_DENOMINATOR
			SyntheticTier::Isolated => None, // DEFAULT_MAX_TWAP_UPDATE_PRICE_BAND_DENOMINATOR
		})
	}
}

/// Stores the state relevant for tracking liquidity mining rewards at the `AMM` level.
/// These values are used in conjunction with `PositionRewardInfo`, `Tick.reward_growths_outside`,
/// and `AMM.reward_last_updated_timestamp` to determine how many rewards are earned by open
/// positions.
#[derive(
	Copy,
	Clone,
	AnchorSerialize,
	AnchorDeserialize,
	Default,
	Debug,
	PartialEq
)]
pub struct AMMRewardInfo {
	/// Reward token mint.
	pub mint: Pubkey,
	/// Reward vault token account.
	pub vault: Pubkey,
	/// Authority account that has permission to initialize the reward and set emissions.
	pub authority: Pubkey,
	/// Q64.64 number that indicates how many tokens per second are earned per unit of liquidity.
	pub emissions_per_second_x64: u128,
	/// Q64.64 number that tracks the total tokens earned per unit of liquidity since the reward
	/// emissions were turned on.
	pub growth_global_x64: u128,
}

impl AMMRewardInfo {
	/// Creates a new `AMMRewardInfo` with the authority set
	pub fn new(authority: Pubkey) -> Self {
		Self {
			authority,
			..Default::default()
		}
	}

	/// Returns true if this reward is initialized.
	/// Once initialized, a reward cannot transition back to uninitialized.
	pub fn initialized(&self) -> bool {
		self.mint.ne(&Pubkey::default())
	}

	/// Maps all reward data to only the reward growth accumulators
	pub fn to_reward_growths(
		reward_infos: &[AMMRewardInfo; NUM_REWARDS]
	) -> [u128; NUM_REWARDS] {
		let mut reward_growths = [0u128; NUM_REWARDS];
		for i in 0..NUM_REWARDS {
			reward_growths[i] = reward_infos[i].growth_global_x64;
		}
		reward_growths
	}
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Default, Copy)]
pub struct AMMBumps {
	pub amm_bump: u8,
}
