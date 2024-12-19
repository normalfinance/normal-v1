use crate::error::{ NormalResult, ErrorCode };
use crate::math::casting::Cast;
use crate::math::constants::{
	ONE_YEAR,
	PERCENTAGE_PRECISION,
	SPOT_RATE_PRECISION,
	SPOT_UTILIZATION_PRECISION,
};
use crate::math::safe_math::{ SafeDivFloor, SafeMath };
use crate::state::oracle::{ OraclePriceData, StrictOraclePrice };
use crate::state::position::Position;
use crate::state::synth_market::SynthMarket;
use crate::state::user::SpotPosition;

pub fn get_spot_balance(
	token_amount: u128,
	spot_market: &SpotMarket,
	balance_type: &SpotBalanceType,
	round_up: bool
) -> NormalResult<u128> {
	let precision_increase = (10_u128).pow(
		(19_u32).safe_sub(spot_market.decimals)?
	);

	let cumulative_interest = match balance_type {
		SpotBalanceType::Deposit => spot_market.cumulative_deposit_interest,
		SpotBalanceType::Borrow => spot_market.cumulative_borrow_interest,
	};

	let mut balance = token_amount
		.safe_mul(precision_increase)?
		.safe_div(cumulative_interest)?;

	if round_up && balance != 0 {
		balance = balance.safe_add(1)?;
	}

	Ok(balance)
}

pub fn get_token_amount(
	balance: u128,
	synth_market: &SynthMarket,
	balance_type: &SpotBalanceType
) -> NormalResult<u128> {
	let precision_decrease = (10_u128).pow(
		(19_u32).safe_sub(synth_market.decimals)?
	);

	let cumulative_interest = match balance_type {
		SpotBalanceType::Deposit => synth_market.cumulative_deposit_interest,
		SpotBalanceType::Borrow => synth_market.cumulative_borrow_interest,
	};

	let token_amount = match balance_type {
		SpotBalanceType::Deposit =>
			balance.safe_mul(cumulative_interest)?.safe_div(precision_decrease)?,
		SpotBalanceType::Borrow =>
			balance.safe_mul(cumulative_interest)?.safe_div_ceil(precision_decrease)?,
	};

	Ok(token_amount)
}

pub fn get_signed_token_amount(
	token_amount: u128,
	balance_type: &SpotBalanceType
) -> NormalResult<i128> {
	match balance_type {
		SpotBalanceType::Deposit => token_amount.cast(),
		SpotBalanceType::Borrow =>
			token_amount.cast::<i128>().map(|token_amount| -token_amount),
	}
}

pub fn get_interest_token_amount(
	balance: u128,
	synth_market: &SynthMarket,
	interest: u128
) -> NormalResult<u128> {
	let precision_decrease = (10_u128).pow(
		(19_u32).safe_sub(synth_market.decimals)?
	);

	let token_amount = balance.safe_mul(interest)?.safe_div(precision_decrease)?;

	Ok(token_amount)
}

pub struct InterestAccumulated {
	pub deposit_interest: u128,
}

pub fn calculate_utilization(
	deposit_token_amount: u128,
	borrow_token_amount: u128
) -> NormalResult<u128> {
	let utilization = borrow_token_amount
		.safe_mul(SPOT_UTILIZATION_PRECISION)?
		.checked_div(deposit_token_amount)
		.unwrap_or({
			if deposit_token_amount == 0 && borrow_token_amount == 0 {
				0_u128
			} else {
				// if there are borrows without deposits, default to maximum utilization rate
				SPOT_UTILIZATION_PRECISION
			}
		});

	Ok(utilization)
}

pub fn calculate_synth_market_utilization(
	synth_market: &SynthMarket
) -> NormalResult<u128> {
	let collateral_token_amount = get_token_amount(
		synth_market.collateral_balance,
		synth_market,
		&SpotBalanceType::Deposit
	)?;
	let debt_token_amount = get_token_amount(
		synth_market.debt_balance,
		synth_market,
		&SpotBalanceType::Borrow
	)?;
	let utilization = calculate_utilization(
		collateral_token_amount,
		debt_token_amount
	)?;

	Ok(utilization)
}

pub fn calculate_accumulated_interest(
	spot_market: &SynthMarket,
	now: i64
) -> NormalResult<InterestAccumulated> {
	if now <= spot_market.last_interest_ts.cast()? {
		return Ok(InterestAccumulated {
			borrow_interest: 0,
			deposit_interest: 0,
		});
	}

	let utilization = calculate_synth_market_utilization(spot_market)?;

	if utilization == 0 {
		return Ok(InterestAccumulated {
			borrow_interest: 0,
			deposit_interest: 0,
		});
	}

	// let borrow_rate = calculate_borrow_rate(spot_market, utilization)?;

	let time_since_last_update = now
		.cast::<u64>()
		.or(Err(ErrorCode::UnableToCastUnixTime))?
		.safe_sub(spot_market.last_interest_ts)?;

	// To save some compute units, have to multiply the rate by the `time_since_last_update` here
	// and then divide out by ONE_YEAR when calculating interest accumulated below
	// let modified_borrow_rate = borrow_rate.safe_mul(
	// 	time_since_last_update as u128
	// )?;

	let modified_collateral_rate = modified_borrow_rate
		.safe_mul(utilization)?
		.safe_div(SPOT_UTILIZATION_PRECISION)?;

	// let borrow_interest = spot_market.cumulative_borrow_interest
	// 	.safe_mul(modified_borrow_rate)?
	// 	.safe_div(ONE_YEAR)?
	// 	.safe_div(SPOT_RATE_PRECISION)?
	// 	.safe_add(1)?;

	let collateral_interest = spot_market.cumulative_collateral_interest
		.safe_mul(modified_collateral_rate)?
		.safe_div(ONE_YEAR)?
		.safe_div(SPOT_RATE_PRECISION)?;

	Ok(InterestAccumulated {
		borrow_interest,
		collateral_interest,
	})
}

// #[cfg(feature = "drift-rs")]
// pub fn calculate_deposit_rate(
// 	spot_market: &SpotMarket,
// 	utilization: u128,
// 	borrow_rate: u128
// ) -> NormalResult<u128> {
// 	borrow_rate
// 		.safe_mul(
// 			PERCENTAGE_PRECISION.safe_sub(
// 				spot_market.insurance_fund.total_factor.cast()?
// 			)?
// 		)?
// 		.safe_mul(utilization)?
// 		.safe_div(SPOT_UTILIZATION_PRECISION)?
// 		.safe_div(PERCENTAGE_PRECISION)
// }

pub fn get_balance_value_and_token_amount(
	position: &Position,
	synth_market: &SynthMarket,
	oracle_price_data: &OraclePriceData
) -> NormalResult<(u128, u128)> {
	let token_amount = position.get_token_amount(synth_market)?;

	let precision_decrease = (10_u128).pow(synth_market.decimals);

	let value = token_amount
		.safe_mul(oracle_price_data.price.cast()?)?
		.safe_div(precision_decrease)?;

	Ok((value, token_amount))
}

pub fn get_strict_token_value(
	token_amount: i128,
	spot_decimals: u32,
	strict_price: &StrictOraclePrice
) -> NormalResult<i128> {
	if token_amount == 0 {
		return Ok(0);
	}

	let precision_decrease = (10_i128).pow(spot_decimals);

	let price = if token_amount > 0 {
		strict_price.min()
	} else {
		strict_price.max()
	};

	let token_with_price = token_amount.safe_mul(price.cast()?)?;

	if token_with_price < 0 {
		token_with_price.safe_div_floor(precision_decrease)
	} else {
		token_with_price.safe_div(precision_decrease)
	}
}

pub fn get_token_value(
	token_amount: i128,
	synth_decimals: u32,
	oracle_price: i64
) -> NormalResult<i128> {
	if token_amount == 0 {
		return Ok(0);
	}

	let precision_decrease = (10_i128).pow(synth_decimals);
	let token_with_oracle = token_amount.safe_mul(oracle_price.cast()?)?;

	if token_with_oracle < 0 {
		token_with_oracle.safe_div_floor(precision_decrease.abs())
	} else {
		token_with_oracle.safe_div(precision_decrease)
	}
}

pub fn get_balance_value(
	position: &Position,
	synth_market: &SynthMarket,
	oracle_price_data: &OraclePriceData
) -> NormalResult<u128> {
	let (value, _) = get_balance_value_and_token_amount(
		position,
		synth_market,
		oracle_price_data
	)?;
	Ok(value)
}
