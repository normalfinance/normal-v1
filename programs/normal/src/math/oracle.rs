use std::cmp::max;

use borsh::{ BorshDeserialize, BorshSerialize };
use solana_program::msg;

use crate::error::{ NormalResult, ErrorCode };
use crate::math::amm;
use crate::math::casting::Cast;
use crate::constants::main::BID_ASK_SPREAD_PRECISION;
use crate::math::safe_math::SafeMath;

use crate::state::oracle::OraclePriceData;
use crate::state::paused_operations::MarketOperation;
use crate::state::market::Market;
use crate::state::state::{ OracleGuardRails, ValidityGuardRails };
use std::fmt;

// #[cfg(test)]
// mod tests;

// ordered by "severity"
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
pub enum OracleValidity {
	NonPositive,
	TooVolatile,
	TooUncertain,
	InsufficientDataPoints,
	StaleForAMM,
	#[default]
	Valid,
}

impl OracleValidity {
	pub fn get_error_code(&self) -> ErrorCode {
		match self {
			OracleValidity::NonPositive => ErrorCode::OracleNonPositive,
			OracleValidity::TooVolatile => ErrorCode::OracleTooVolatile,
			OracleValidity::TooUncertain => ErrorCode::OracleTooUncertain,
			OracleValidity::InsufficientDataPoints =>
				ErrorCode::OracleInsufficientDataPoints,
			OracleValidity::StaleForAMM => ErrorCode::OracleStaleForAMM,
			OracleValidity::Valid => unreachable!(),
		}
	}
}

impl fmt::Display for OracleValidity {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			OracleValidity::NonPositive => write!(f, "NonPositive"),
			OracleValidity::TooVolatile => write!(f, "TooVolatile"),
			OracleValidity::TooUncertain => write!(f, "TooUncertain"),
			OracleValidity::InsufficientDataPoints =>
				write!(f, "InsufficientDataPoints"),
			OracleValidity::StaleForAMM => write!(f, "StaleForAMM"),
			OracleValidity::Valid => write!(f, "Valid"),
		}
	}
}

#[derive(Clone, Copy, BorshSerialize, BorshDeserialize, PartialEq, Debug, Eq)]
pub enum NormalAction {
	SettlePnl,
	TriggerOrder,
	FillOrderMatch,
	FillOrderAmm,
	UpdateTwap,
	UpdateAMMCurve,
	OracleOrderPrice,
}

pub fn is_oracle_valid_for_action(
	oracle_validity: OracleValidity,
	action: Option<NormalAction>
) -> NormalResult<bool> {
	let is_ok = match action {
		Some(action) =>
			match action {
				NormalAction::FillOrderAmm => {
					matches!(oracle_validity, OracleValidity::Valid)
				}
				NormalAction::OracleOrderPrice => {
					matches!(
						oracle_validity,
						OracleValidity::Valid |
							OracleValidity::StaleForAMM |
							OracleValidity::InsufficientDataPoints
					)
				}
				NormalAction::TriggerOrder =>
					!matches!(
						oracle_validity,
						OracleValidity::NonPositive | OracleValidity::TooVolatile
					),
				NormalAction::FillOrderMatch =>
					!matches!(
						oracle_validity,
						OracleValidity::NonPositive |
							OracleValidity::TooVolatile |
							OracleValidity::TooUncertain
					),
				NormalAction::UpdateTwap =>
					!matches!(oracle_validity, OracleValidity::NonPositive),
				NormalAction::UpdateAMMCurve =>
					!matches!(oracle_validity, OracleValidity::NonPositive),
			}
		None => { matches!(oracle_validity, OracleValidity::Valid) }
	};

	Ok(is_ok)
}

pub fn block_operation(
	market: &Market,
	oracle_price_data: &OraclePriceData,
	guard_rails: &OracleGuardRails,
	reserve_price: u64,
	slot: u64
) -> NormalResult<bool> {
	let OracleStatus {
		oracle_validity,
		mark_too_divergent: is_oracle_mark_too_divergent,
		oracle_reserve_price_spread_pct: _,
		..
	} = get_oracle_status(market, oracle_price_data, guard_rails, reserve_price)?;

	let block = is_oracle_mark_too_divergent;
	Ok(block)
}

#[derive(Default, Clone, Copy, Debug)]
pub struct OracleStatus {
	pub price_data: OraclePriceData,
	pub oracle_reserve_price_spread_pct: i64,
	pub mark_too_divergent: bool,
	pub oracle_validity: OracleValidity,
}

pub fn get_oracle_status(
	market: &Market,
	oracle_price_data: &OraclePriceData,
	guard_rails: &OracleGuardRails,
	reserve_price: u64
) -> NormalResult<OracleStatus> {
	let oracle_validity = oracle_validity(
		market.market_index,
		amm.historical_oracle_data.last_oracle_price_twap,
		oracle_price_data,
		&guard_rails.validity,
		market.get_max_confidence_interval_multiplier()?,
		false
	)?;
	let oracle_reserve_price_spread_pct =
		amm::calculate_oracle_twap_5min_price_spread_pct(
			&market.amm,
			reserve_price
		)?;
	let is_oracle_mark_too_divergent = amm::is_oracle_mark_too_divergent(
		oracle_reserve_price_spread_pct,
		&guard_rails.price_divergence
	)?;

	Ok(OracleStatus {
		price_data: *oracle_price_data,
		oracle_reserve_price_spread_pct,
		mark_too_divergent: is_oracle_mark_too_divergent,
		oracle_validity,
	})
}

pub fn oracle_validity(
	market_index: u16,
	last_oracle_twap: i64,
	oracle_price_data: &OraclePriceData,
	valid_oracle_guard_rails: &ValidityGuardRails,
	max_confidence_interval_multiplier: u64,
	log_validity: bool
) -> NormalResult<OracleValidity> {
	let OraclePriceData {
		price: oracle_price,
		confidence: oracle_conf,
		delay: oracle_delay,
		has_sufficient_number_of_data_points,
		..
	} = *oracle_price_data;

	let is_oracle_price_nonpositive = oracle_price <= 0;

	let is_oracle_price_too_volatile = oracle_price
		.max(last_oracle_twap)
		.safe_div(last_oracle_twap.min(oracle_price).max(1))?
		.gt(&valid_oracle_guard_rails.too_volatile_ratio);

	let conf_pct_of_price = max(1, oracle_conf)
		.safe_mul(BID_ASK_SPREAD_PRECISION)?
		.safe_div(oracle_price.cast()?)?;

	// TooUncertain
	let is_conf_too_large = conf_pct_of_price.gt(
		&valid_oracle_guard_rails.confidence_interval_max_size.safe_mul(
			max_confidence_interval_multiplier
		)?
	);

	let is_stale_for_amm = oracle_delay.gt(
		&valid_oracle_guard_rails.slots_before_stale_for_amm
	);

	let oracle_validity = if is_oracle_price_nonpositive {
		OracleValidity::NonPositive
	} else if is_oracle_price_too_volatile {
		OracleValidity::TooVolatile
	} else if is_conf_too_large {
		OracleValidity::TooUncertain
	} else if !has_sufficient_number_of_data_points {
		OracleValidity::InsufficientDataPoints
	} else if is_stale_for_amm {
		OracleValidity::StaleForAMM
	} else {
		OracleValidity::Valid
	};

	if log_validity {
		if !has_sufficient_number_of_data_points {
			msg!(
				"Invalid {} {} Oracle: Insufficient Data Points",
				market_type,
				market_index
			);
		}

		if is_oracle_price_nonpositive {
			msg!(
				"Invalid {} {} Oracle: Non-positive (oracle_price <=0)",
				market_type,
				market_index
			);
		}

		if is_oracle_price_too_volatile {
			msg!(
				"Invalid {} {} Oracle: Too Volatile (last_oracle_price_twap={:?} vs oracle_price={:?})",
				market_type,
				market_index,
				last_oracle_twap,
				oracle_price
			);
		}

		if is_conf_too_large {
			msg!(
				"Invalid {} {} Oracle: Confidence Too Large (is_conf_too_large={:?})",
				market_type,
				market_index,
				conf_pct_of_price
			);
		}

		if is_stale_for_amm {
			msg!(
				"Invalid {} {} Oracle: Stale (oracle_delay={:?})",
				market_type,
				market_index,
				oracle_delay
			);
		}
	}

	Ok(oracle_validity)
}

pub fn get_timestamp_from_price_feed_account(
	price_feed_account: &AccountInfo
) -> Result<i64> {
	if price_feed_account.data_is_empty() {
		Ok(0)
	} else {
		let price_feed_account_data = price_feed_account.try_borrow_data()?;
		let price_feed_account = PriceUpdateV2::try_deserialize(
			&mut &price_feed_account_data[..]
		)?;
		Ok(price_feed_account.price_message.publish_time)
	}
}

pub fn get_timestamp_from_price_update_message(
	update_message: &PrefixedVec<u16, u8>
) -> Result<i64> {
	let message = from_slice::<byteorder::BE, Message>(
		update_message.as_ref()
	).map_err(|_| ErrorCode::OracleDeserializeMessageFailed)?;
	let next_timestamp = match message {
		Message::PriceFeedMessage(price_feed_message) =>
			price_feed_message.publish_time,
		Message::TwapMessage(_) => {
			return Err(ErrorCode::OracleUnsupportedMessageType.into());
		}
	};
	Ok(next_timestamp)
}
