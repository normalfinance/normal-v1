import { MarketAccount, BalanceType, isVariant } from '../types';
import { BN } from '@coral-xyz/anchor';
import { ONE, TEN, ZERO } from '../constants/numericConstants';
import { OraclePriceData } from '../oracles/types';
import { divCeil } from './utils';
import { StrictOraclePrice } from '../oracles/strictOraclePrice';

/**
 * Calculates the balance of a given token amount including any accumulated interest. This
 * is the same as `SpotPosition.scaledBalance`.
 *
 * @param {BN} tokenAmount - the amount of tokens
 * @param {SpotMarketAccount} market - the spot market account
 * @param {BalanceType} balanceType - the balance type ('deposit' or 'borrow')
 * @return {BN} the calculated balance, scaled by `SPOT_MARKET_BALANCE_PRECISION`
 */
export function getBalance(
	tokenAmount: BN,
	market: MarketAccount,
	balanceType: BalanceType
): BN {
	const precisionIncrease = TEN.pow(new BN(19 - market.decimals));

	// const cumulativeInterest = isVariant(balanceType, 'deposit')
	// 	? market.cumulativeDepositInterest
	// 	: market.cumulativeBorrowInterest;

	let balance = tokenAmount.mul(precisionIncrease).div(new BN(1)); // cumulativeInterest

	if (!balance.eq(ZERO) && isVariant(balanceType, 'borrow')) {
		balance = balance.add(ONE);
	}

	return balance;
}

/**
 * Calculates the spot token amount including any accumulated interest.
 *
 * @param {BN} balanceAmount - The balance amount, typically from `SpotPosition.scaledBalance`
 * @param {SpotMarketAccount} market - The spot market account details
 * @param {BalanceType} balanceType - The balance type to be used for calculation
 * @returns {BN} The calculated token amount, scaled by `SpotMarketConfig.precision`
 */
export function getTokenAmount(
	balanceAmount: BN,
	market: MarketAccount,
	balanceType: BalanceType
): BN {
	const precisionDecrease = TEN.pow(new BN(19 - market.decimals));

	if (isVariant(balanceType, 'deposit')) {
		return balanceAmount
			.mul(new BN(1)) // market.cumulativeDepositInterest
			.div(precisionDecrease);
	} else {
		return divCeil(
			balanceAmount.mul(new BN(1)), // market.cumulativeBorrowInterest
			precisionDecrease
		);
	}
}

/**
 * Returns the signed (positive for deposit,negative for borrow) token amount based on the balance type.
 *
 * @param {BN} tokenAmount - The token amount to convert (from `getTokenAmount`)
 * @param {BalanceType} balanceType - The balance type to determine the sign of the token amount.
 * @returns {BN} - The signed token amount, scaled by `SpotMarketConfig.precision`
 */
export function getSignedTokenAmount(
	tokenAmount: BN,
	balanceType: BalanceType
): BN {
	if (isVariant(balanceType, 'deposit')) {
		return tokenAmount;
	} else {
		return tokenAmount.abs().neg();
	}
}

/**
 * Calculates the value of a given token amount using the worst of the provided oracle price and its TWAP.
 *
 * @param {BN} tokenAmount - The amount of tokens to calculate the value for (from `getTokenAmount`)
 * @param {number} spotDecimals - The number of decimals in the token.
 * @param {StrictOraclePrice} strictOraclePrice - Contains oracle price and 5min twap.
 * @return {BN} The calculated value of the given token amount, scaled by `PRICE_PRECISION`
 */
export function getStrictTokenValue(
	tokenAmount: BN,
	spotDecimals: number,
	strictOraclePrice: StrictOraclePrice
): BN {
	if (tokenAmount.eq(ZERO)) {
		return ZERO;
	}

	let price;
	if (tokenAmount.gte(ZERO)) {
		price = strictOraclePrice.min();
	} else {
		price = strictOraclePrice.max();
	}

	const precisionDecrease = TEN.pow(new BN(spotDecimals));

	return tokenAmount.mul(price).div(precisionDecrease);
}

/**
 * Calculates the value of a given token amount in relation to an oracle price data
 *
 * @param {BN} tokenAmount - The amount of tokens to calculate the value for (from `getTokenAmount`)
 * @param {number} spotDecimals - The number of decimal places of the token.
 * @param {OraclePriceData} oraclePriceData - The oracle price data (typically a token/USD oracle).
 * @return {BN} The value of the token based on the oracle, scaled by `PRICE_PRECISION`
 */
export function getTokenValue(
	tokenAmount: BN,
	spotDecimals: number,
	oraclePriceData: Pick<OraclePriceData, 'price'>
): BN {
	if (tokenAmount.eq(ZERO)) {
		return ZERO;
	}

	const precisionDecrease = TEN.pow(new BN(spotDecimals));

	return tokenAmount.mul(oraclePriceData.price).div(precisionDecrease);
}

// ...
