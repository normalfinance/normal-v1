use std::fmt;
use std::fmt::{ Display, Formatter };

use anchor_lang::prelude::*;
use borsh::{ BorshDeserialize, BorshSerialize };

use crate::error::{ NormalResult, ErrorCode };
use crate::math::casting::Cast;
use crate::math::constants::{ AMM_RESERVE_PRECISION, FIVE_MINUTE, ONE_HOUR };
#[cfg(test)]
use crate::math::constants::PRICE_PRECISION_I64;
use crate::math::safe_math::SafeMath;

use crate::math::stats::calculate_new_twap;
use crate::state::oracle::{
	HistoricalIndexData,
	HistoricalOracleData,
	OracleSource,
};
use crate::state::insurance::InsuranceClaim;
use crate::state::paused_operations::{ Operation, InsuranceFundOperation };
use crate::state::traits::{ MarketIndexOffset, Size };
use crate::{ validate, PERCENTAGE_PRECISION };

use crate::state::amm::AMM;

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
	/// fills only able to reduce liability
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
	Default
)]
pub enum SyntheticType {
	#[default]
	Asset,
	Index,
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
	pub fn is_as_safe_as(&self, best_contract: &SyntheticTier) -> bool {
		self.is_as_safe_as_contract(best_contract)
	}

	pub fn is_as_safe_as_contract(&self, other: &SyntheticTier) -> bool {
		// Contract Tier A safest
		self <= other
	}
}

#[account(zero_copy(unsafe))]
#[derive(PartialEq, Eq, Debug)]
#[repr(C)]
pub struct Market {
    /// The address of the market. It is a pda of the market index
    pub pubkey: Pubkey,
    /// The oracle used to price the markets deposits/borrows
    pub oracle: Pubkey,
    /// The automated market maker
    pub amm: AMM,
    /// The token mint of the market
    pub mint: Pubkey,
    /// The vault used to store the market's deposits
    /// The amount in the vault should be equal to or greater than deposits - borrows
    pub vault: Pubkey,
    /// The encoded display name for the market e.g. SOL
    pub name: [u8; 32],
    pub historical_oracle_data: HistoricalOracleData,
    pub historical_index_data: HistoricalIndexData,
    /// Revenue the protocol has collected in the quote asset (i.e. SOL or USDC)
    pub fee_pool: PoolBalance,
    /// The total spot fees collected for this market
    /// precision: QUOTE_PRECISION
    pub total_fee: u128,
    /// The time the market is set to expire. Only set if market is in reduce only mode
    pub expiry_ts: i64,
    /// Spot orders must be a multiple of the step size
    /// precision: token mint precision
    pub order_step_size: u64,
    /// Spot orders must be a multiple of the tick size
    /// precision: PRICE_PRECISION
    pub order_tick_size: u64,
    /// The minimum order size
    /// precision: token mint precision
    pub min_order_size: u64,
    /// The maximum spot position size
    /// if the limit is 0, there is no limit
    /// precision: token mint precision
    pub max_position_size: u64,
    /// Every spot trade has a fill record id. This is the next id to use
    pub next_fill_record_id: u64,

    /// The market's token mint's decimals. To from decimals to a precision, 10^decimals
    pub decimals: u32,
    pub market_index: u16,
    pub oracle_source: OracleSource,
    pub status: MarketStatus,
    /// The asset tier affects how a deposit can be used as collateral and the priority for a borrow being liquidated
    pub synthetic_tier: SyntheticTier,
    pub paused_operations: u8,
    pub fee_adjustment: i16,

    pub token_program: u8,

    // FROM PERP
    /// number of users in a position (base)
    pub number_of_users_with_base: u32,
    /// number of users in a position (pnl) or pnl (quote)
    pub number_of_users: u32,

    pub padding: [u8; 41],
}

impl Default for Market {
    fn default() -> Self {
        Market {
            pubkey: Pubkey::default(),
            oracle: Pubkey::default(),
            amm: AMM::default(),
            mint: Pubkey::default(),
            vault: Pubkey::default(),
            name: [0; 32],
            historical_oracle_data: HistoricalOracleData::default(),
            historical_index_data: HistoricalIndexData::default(),
            fee_pool: PoolBalance::default(),
            total_fee: 0,
            expiry_ts: 0,
            order_step_size: 1,
            order_tick_size: 0,
            min_order_size: 0,
            max_position_size: 0,
            next_fill_record_id: 0,
            decimals: 0,
            market_index: 0,
            oracle_source: OracleSource::default(),
            status: MarketStatus::default(),
            synthetic_tier: SyntheticTier::default(),
            paused_operations: 0,
            fee_adjustment: 0,

            token_program: 0,
            // ...
            number_of_users_with_base: 0,
            number_of_users: 0,
            // ...
            padding: [0; 41],
        }
    }
}

impl Size for Market {
    const SIZE: usize = 776;
}

impl MarketIndexOffset for Market {
    const MARKET_INDEX_OFFSET: usize = 684;
}

impl Market {
    pub fn is_in_settlement(&self, now: i64) -> bool {
        let in_settlement = matches!(
            self.status,
            MarketStatus::Settlement | MarketStatus::Delisted
        );
        let expired = self.expiry_ts != 0 && now >= self.expiry_ts;
        in_settlement || expired
    }

    pub fn is_reduce_only(&self) -> bool {
        self.status == MarketStatus::ReduceOnly
    }

    pub fn is_operation_paused(&self, operation: SpotOperation) -> bool {
        SpotOperation::is_operation_paused(self.paused_operations, operation)
    }

    pub fn fills_enabled(&self) -> bool {
        matches!(self.status, MarketStatus::Active | MarketStatus::ReduceOnly) &&
            !self.is_operation_paused(SpotOperation::Fill)
    }

    pub fn get_max_confidence_interval_multiplier(&self) -> NormalResult<u64> {
        Ok(match self.synthetic_tier {
            SyntheticTier::Collateral => 1, // 2%
            SyntheticTier::Protected => 1, // 2%
            SyntheticTier::Cross => 5, // 20%
            SyntheticTier::Isolated => 50, // 100%
            SyntheticTier::Unlisted => 50,
        })
    }

    pub fn get_sanitize_clamp_denominator(&self) -> NormalResult<Option<i64>> {
        Ok(match self.synthetic_tier {
            SyntheticTier::Collateral => Some(10), // 10%
            SyntheticTier::Protected => Some(10), // 10%
            SyntheticTier::Cross => Some(5), // 20%
            SyntheticTier::Isolated => Some(3), // 50%
            SyntheticTier::Unlisted => None, // DEFAULT_MAX_TWAP_UPDATE_PRICE_BAND_DENOMINATOR
        })
    }

    pub fn get_precision(self) -> u64 {
        (10_u64).pow(self.decimals)
    }

    pub fn update_historical_index_price(
        &mut self,
        best_bid: Option<u64>,
        best_ask: Option<u64>,
        now: i64
    ) -> NormalResult {
        let mut mid_price = 0;
        if let Some(best_bid) = best_bid {
            self.historical_index_data.last_index_bid_price = best_bid;
            mid_price += best_bid;
        }

        if let Some(best_ask) = best_ask {
            self.historical_index_data.last_index_ask_price = best_ask;
            mid_price = if mid_price == 0 {
                best_ask
            } else {
                mid_price.safe_add(best_ask)?.safe_div(2)?
            };
        }

        self.historical_index_data.last_index_price_twap = calculate_new_twap(
            mid_price.cast()?,
            now,
            self.historical_index_data.last_index_price_twap.cast()?,
            self.historical_index_data.last_index_price_twap_ts,
            ONE_HOUR
        )?.cast()?;

        self.historical_index_data.last_index_price_twap_5min = calculate_new_twap(
            mid_price.cast()?,
            now,
            self.historical_index_data.last_index_price_twap_5min.cast()?,
            self.historical_index_data.last_index_price_twap_ts,
            FIVE_MINUTE as i64
        )?.cast()?;

        self.historical_index_data.last_index_price_twap_ts = now;

        Ok(())
    }
}

#[cfg(test)]
impl Market {
    pub fn default_base_market() -> Self {
        Market {
            market_index: 1,
            decimals: 9,
            order_step_size: 1,
            order_tick_size: 1,
            status: MarketStatus::Active,
            ..SpotMarket::default()
        }
    }

    pub fn default_quote_market() -> Self {
        Market {
            decimals: 6,
            order_tick_size: 1,
            status: MarketStatus::Active,
            historical_oracle_data: HistoricalOracleData {
                last_oracle_price_twap: PRICE_PRECISION_I64,
                last_oracle_price_twap_5min: PRICE_PRECISION_I64,
                ..HistoricalOracleData::default()
            },
			..Market::default()
		}
	}
}

#[derive(
	Clone,
	Copy,
	BorshSerialize,
	BorshDeserialize,
	PartialEq,
	Eq,
	Debug,
	Default
)]
pub enum BalanceType {
	#[default]
	Deposit,
	Borrow,
}

impl Display for BalanceType {
	fn fmt(&self, f: &mut Formatter) -> fmt::Result {
		match self {
			BalanceType::Deposit => write!(f, "BalanceType::Deposit"),
			BalanceType::Borrow => write!(f, "BalanceType::Borrow"),
        }
    }
}

pub trait Balance {
    fn market_index(&self) -> u16;

	fn balance_type(&self) -> &BalanceType;

    fn balance(&self) -> u128;

    fn increase_balance(&mut self, delta: u128) -> NormalResult;

    fn decrease_balance(&mut self, delta: u128) -> NormalResult;

	fn update_balance_type(&mut self, balance_type: BalanceType) -> NormalResult;
}

#[zero_copy(unsafe)]
#[derive(Default, Eq, PartialEq, Debug)]
#[repr(C)]
pub struct PoolBalance {
    /// precision: SPOT_BALANCE_PRECISION
    pub balance: u128,
	/// The market the pool is for
    pub market_index: u16,
    pub padding: [u8; 6],
}

impl Balance for PoolBalance {
    fn market_index(&self) -> u16 {
        self.market_index
    }

	fn balance_type(&self) -> &BalanceType {
		&BalanceType::Deposit
	}

    fn balance(&self) -> u128 {
        self.balance
    }

    fn increase_balance(&mut self, delta: u128) -> NormalResult {
        self.balance = self.balance.safe_add(delta)?;
        Ok(())
    }

    fn decrease_balance(&mut self, delta: u128) -> NormalResult {
        self.balance = self.balance.safe_sub(delta)?;
        Ok(())
    }

	fn update_balance_type(
		&mut self,
		_balance_type: BalanceType
	) -> NormalResult {
		Err(ErrorCode::CantUpdatePoolBalanceType)
	}
}
