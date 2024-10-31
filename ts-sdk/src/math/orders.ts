import { User } from '../user';
import {
	isOneOfVariant,
	isVariant,
	MarketAccount,
	AMM,
	Order,
	OrderSide,
} from '../types';
import { ZERO, TWO, ONE } from '../constants/numericConstants';
import { BN } from '@coral-xyz/anchor';
import { OraclePriceData } from '../oracles/types';
import {
	getAuctionPrice,
	isAuctionComplete,
	isFallbackAvailableLiquiditySource,
} from './auction';
import {
	calculateMaxBaseAssetAmountFillable,
	calculateMaxBaseAssetAmountToTrade,
	calculateUpdatedAMM,
} from './amm';

export function isOrderRiskIncreasing(user: User, order: Order): boolean {
	if (isVariant(order.status, 'init')) {
		return false;
	}

	const position =
		user.getPosition(order.marketIndex) ||
		user.getEmptyPosition(order.marketIndex);

	// if no position exists, it's risk increasing
	if (position.baseAssetAmount.eq(ZERO)) {
		return true;
	}

	// if position is buy and order is buy
	if (position.baseAssetAmount.gt(ZERO) && isVariant(order.side, 'buy')) {
		return true;
	}

	// if position is sell and order is sell
	if (position.baseAssetAmount.lt(ZERO) && isVariant(order.side, 'sell')) {
		return true;
	}

	const baseAssetAmountToFill = order.baseAssetAmount.sub(
		order.baseAssetAmountFilled
	);
	// if order will flip position
	if (baseAssetAmountToFill.gt(position.baseAssetAmount.abs().mul(TWO))) {
		return true;
	}

	return false;
}

export function isOrderRiskIncreasingInSameDirection(
	user: User,
	order: Order
): boolean {
	if (isVariant(order.status, 'init')) {
		return false;
	}

	const position =
		user.getPosition(order.marketIndex) ||
		user.getEmptyPosition(order.marketIndex);

	// if no position exists, it's risk increasing
	if (position.baseAssetAmount.eq(ZERO)) {
		return true;
	}

	// if position is buy and order is buy
	if (position.baseAssetAmount.gt(ZERO) && isVariant(order.side, 'buy')) {
		return true;
	}

	// if position is sell and order is sell
	if (position.baseAssetAmount.lt(ZERO) && isVariant(order.side, 'sell')) {
		return true;
	}

	return false;
}

export function isOrderReduceOnly(user: User, order: Order): boolean {
	if (isVariant(order.status, 'init')) {
		return false;
	}

	const position =
		user.getPosition(order.marketIndex) ||
		user.getEmptyPosition(order.marketIndex);

	// if position is buy and order is buy
	if (position.baseAssetAmount.gte(ZERO) && isVariant(order.side, 'buy')) {
		return false;
	}

	// if position is sell and order is sell
	if (position.baseAssetAmount.lte(ZERO) && isVariant(order.side, 'sell')) {
		return false;
	}

	return true;
}

export function standardizeBaseAssetAmount(
	baseAssetAmount: BN,
	stepSize: BN
): BN {
	const remainder = baseAssetAmount.mod(stepSize);
	return baseAssetAmount.sub(remainder);
}

export function standardizePrice(
	price: BN,
	tickSize: BN,
	direction: OrderSide
): BN {
	if (price.eq(ZERO)) {
		console.log('price is zero');
		return price;
	}

	const remainder = price.mod(tickSize);
	if (remainder.eq(ZERO)) {
		return price;
	}

	if (isVariant(direction, 'buy')) {
		return price.sub(remainder);
	} else {
		return price.add(tickSize).sub(remainder);
	}
}

export function getLimitPrice(
	order: Order,
	oraclePriceData: OraclePriceData,
	slot: number,
	fallbackPrice?: BN
): BN | undefined {
	let limitPrice;
	if (hasAuctionPrice(order, slot)) {
		limitPrice = getAuctionPrice(order, slot, oraclePriceData.price);
	} else if (order.oraclePriceOffset !== 0) {
		limitPrice = BN.max(
			oraclePriceData.price.add(new BN(order.oraclePriceOffset)),
			ONE
		);
	} else if (order.price.eq(ZERO)) {
		limitPrice = fallbackPrice;
	} else {
		limitPrice = order.price;
	}

	return limitPrice;
}

export function hasLimitPrice(order: Order, slot: number): boolean {
	return (
		order.price.gt(ZERO) ||
		order.oraclePriceOffset != 0 ||
		!isAuctionComplete(order, slot)
	);
}

export function hasAuctionPrice(order: Order, slot: number): boolean {
	return (
		!isAuctionComplete(order, slot) &&
		(!order.auctionStartPrice.eq(ZERO) || !order.auctionEndPrice.eq(ZERO))
	);
}

export function isFillableByVAMM(
	order: Order,
	market: MarketAccount,
	oraclePriceData: OraclePriceData,
	slot: number,
	ts: number,
	minAuctionDuration: number
): boolean {
	return (
		(isFallbackAvailableLiquiditySource(order, minAuctionDuration, slot) &&
			calculateBaseAssetAmountForAmmToFulfill(
				order,
				market,
				oraclePriceData,
				slot
			).gte(market.amm.minOrderSize)) ||
		isOrderExpired(order, ts)
	);
}

export function calculateBaseAssetAmountForAmmToFulfill(
	order: Order,
	market: MarketAccount,
	oraclePriceData: OraclePriceData,
	slot: number
): BN {
	if (mustBeTriggered(order) && !isTriggered(order)) {
		return ZERO;
	}

	const limitPrice = getLimitPrice(order, oraclePriceData, slot);
	let baseAssetAmount;

	const updatedAMM = calculateUpdatedAMM(market.amm, oraclePriceData);
	if (limitPrice !== undefined) {
		baseAssetAmount = calculateBaseAssetAmountToFillUpToLimitPrice(
			order,
			updatedAMM,
			limitPrice,
			oraclePriceData
		);
	} else {
		baseAssetAmount = order.baseAssetAmount.sub(order.baseAssetAmountFilled);
	}

	const maxBaseAssetAmount = calculateMaxBaseAssetAmountFillable(
		updatedAMM,
		order.side
	);

	return BN.min(maxBaseAssetAmount, baseAssetAmount);
}

export function calculateBaseAssetAmountToFillUpToLimitPrice(
	order: Order,
	amm: AMM,
	limitPrice: BN,
	oraclePriceData: OraclePriceData
): BN {
	const adjustedLimitPrice = isVariant(order.side, 'buy')
		? limitPrice.sub(amm.orderTickSize)
		: limitPrice.add(amm.orderTickSize);

	const [maxAmountToTrade, direction] = calculateMaxBaseAssetAmountToTrade(
		amm,
		adjustedLimitPrice,
		order.side,
		oraclePriceData
	);

	const baseAssetAmount = standardizeBaseAssetAmount(
		maxAmountToTrade,
		amm.orderStepSize
	);

	// Check that directions are the same
	const sameDirection = isSameDirection(direction, order.side);
	if (!sameDirection) {
		return ZERO;
	}

	const baseAssetAmountUnfilled = order.baseAssetAmount.sub(
		order.baseAssetAmountFilled
	);
	return baseAssetAmount.gt(baseAssetAmountUnfilled)
		? baseAssetAmountUnfilled
		: baseAssetAmount;
}

function isSameDirection(
	firstDirection: OrderSide,
	secondDirection: OrderSide
): boolean {
	return (
		(isVariant(firstDirection, 'buy') && isVariant(secondDirection, 'buy')) ||
		(isVariant(firstDirection, 'sell') && isVariant(secondDirection, 'sell'))
	);
}

export function isOrderExpired(
	order: Order,
	ts: number,
	enforceBuffer = false,
	bufferSeconds = 15
): boolean {
	if (
		mustBeTriggered(order) ||
		!isVariant(order.status, 'open') ||
		order.maxTs.eq(ZERO)
	) {
		return false;
	}

	let maxTs;
	if (enforceBuffer && isLimitOrder(order)) {
		maxTs = order.maxTs.addn(bufferSeconds);
	} else {
		maxTs = order.maxTs;
	}

	return new BN(ts).gt(maxTs);
}

export function isMarketOrder(order: Order): boolean {
	return isOneOfVariant(order.orderType, ['market', 'triggerMarket']);
}

export function isLimitOrder(order: Order): boolean {
	return isOneOfVariant(order.orderType, ['limit', 'triggerLimit']);
}

export function mustBeTriggered(order: Order): boolean {
	return isOneOfVariant(order.orderType, ['triggerMarket', 'triggerLimit']);
}

export function isTriggered(order: Order): boolean {
	return isOneOfVariant(order.triggerCondition, [
		'triggeredAbove',
		'triggeredBelow',
	]);
}

export function isRestingLimitOrder(order: Order, slot: number): boolean {
	if (!isLimitOrder(order)) {
		return false;
	}

	return order.postOnly || isAuctionComplete(order, slot);
}

export function isTakingOrder(order: Order, slot: number): boolean {
	return isMarketOrder(order) || !isRestingLimitOrder(order, slot);
}
