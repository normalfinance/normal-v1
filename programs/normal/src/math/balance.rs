use crate::error::{ NormalResult, ErrorCode };
use crate::math::casting::Cast;
use crate::math::constants::{ ONE_YEAR, PERCENTAGE_PRECISION };
use crate::math::safe_math::{ SafeDivFloor, SafeMath };
use crate::state::oracle::{ OraclePriceData, StrictOraclePrice };
use crate::state::market::{ BalanceType, Market };
use crate::state::user::Position;

pub fn get_balance(
	token_amount: u128,
	market: &Market,
	balance_type: &BalanceType,
	round_up: bool
) -> NormalResult<u128> {
	let precision_increase = (10_u128).pow((19_u32).safe_sub(market.decimals)?);

	// let cumulative_interest = match balance_type {
	// 	BalanceType::Deposit => market.cumulative_deposit_interest,
	// 	BalanceType::Borrow => market.cumulative_borrow_interest,
	// };

	let mut balance = token_amount.safe_mul(precision_increase)?.safe_div(1)?;

	if round_up && balance != 0 {
		balance = balance.safe_add(1)?;
	}

	Ok(balance)
}

pub fn get_token_amount(
	balance: u128,
	market: &Market,
	balance_type: &BalanceType
) -> NormalResult<u128> {
	let precision_decrease = (10_u128).pow((19_u32).safe_sub(market.decimals)?);

	let token_amount = match balance_type {
		BalanceType::Deposit => balance.safe_mul(1)?.safe_div(precision_decrease)?,
		BalanceType::Borrow =>
			balance.safe_mul(1)?.safe_div_ceil(precision_decrease)?,
	};

	Ok(token_amount)
}

pub fn get_signed_token_amount(
	token_amount: u128,
	balance_type: &BalanceType
) -> NormalResult<i128> {
	match balance_type {
		BalanceType::Deposit => token_amount.cast(),
		BalanceType::Borrow =>
			token_amount.cast::<i128>().map(|token_amount| -token_amount),
	}
}

pub fn get_interest_token_amount(
	balance: u128,
	market: &Market,
	interest: u128
) -> NormalResult<u128> {
	let precision_decrease = (10_u128).pow((19_u32).safe_sub(market.decimals)?);

	let token_amount = balance.safe_mul(interest)?.safe_div(precision_decrease)?;

	Ok(token_amount)
}

pub fn get_balance_value_and_token_amount(
	position: &Position,
	market: &Market,
	oracle_price_data: &OraclePriceData
) -> NormalResult<(u128, u128)> {
	let token_amount = position.get_token_amount(market)?;

	let precision_decrease = (10_u128).pow(market.decimals);

	let value = token_amount
		.safe_mul(oracle_price_data.price.cast()?)?
		.safe_div(precision_decrease)?;

	Ok((value, token_amount))
}

pub fn get_strict_token_value(
	token_amount: i128,
	decimals: u32,
	strict_price: &StrictOraclePrice
) -> NormalResult<i128> {
	if token_amount == 0 {
		return Ok(0);
	}

	let precision_decrease = (10_i128).pow(decimals);

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
	decimals: u32,
	oracle_price: i64
) -> NormalResult<i128> {
	if token_amount == 0 {
		return Ok(0);
	}

	let precision_decrease = (10_i128).pow(decimals);
	let token_with_oracle = token_amount.safe_mul(oracle_price.cast()?)?;

	if token_with_oracle < 0 {
		token_with_oracle.safe_div_floor(precision_decrease.abs())
	} else {
		token_with_oracle.safe_div(precision_decrease)
	}
}

pub fn get_balance_value(
	position: &Position,
	market: &Market,
	oracle_price_data: &OraclePriceData
) -> NormalResult<u128> {
	let (value, _) = get_balance_value_and_token_amount(
		position,
		market,
		oracle_price_data
	)?;
	Ok(value)
}