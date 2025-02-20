import {
	DEFAULT_REVENUE_SINCE_LAST_FUNDING_SPREAD_RETREAT,
	PERCENTAGE_PRECISION,
	ONE,
} from '../constants/numericConstants';
import {
	ExchangeStatus,
	MarketAccount,
	MarketOperation,
	StateAccount,
	isVariant,
	InsuranceFundOperation,
} from '../types';
import { BN } from '@coral-xyz/anchor';

export function exchangePaused(state: StateAccount): boolean {
	return state.exchangeStatus !== ExchangeStatus.ACTIVE;
}

// export function fillPaused(
// 	state: StateAccount,
// 	market: MarketAccount
// ): boolean {
// 	if (
// 		(state.exchangeStatus & ExchangeStatus.FILL_PAUSED) ===
// 		ExchangeStatus.FILL_PAUSED
// 	) {
// 		return true;
// 	}

// 	return isOperationPaused(market.pausedOperations, MarketOperation.FILL);
// }

export function ammPaused(state: StateAccount, market: MarketAccount): boolean {
	if (
		(state.exchangeStatus & ExchangeStatus.AMM_PAUSED) ===
		ExchangeStatus.AMM_PAUSED
	) {
		return true;
	}

	// // const operationPaused = isOperationPaused(
	// // 	market.pausedOperations,
	// // 	MarketOperation.AMM_FILL
	// // );
	// if (operationPaused) {
	// 	return true;
	// }

	return false;
}

export function isOperationPaused(
	pausedOperations: number,
	operation: MarketOperation | InsuranceFundOperation
): boolean {
	return (pausedOperations & operation) > 0;
}
