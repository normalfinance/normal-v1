import {
	Keypair,
	PublicKey,
	Transaction,
	VersionedTransaction,
} from '@solana/web3.js';
import { BN, ZERO } from '.';

// Utility type which lets you denote record with values of type A mapped to a record with the same keys but values of type B
export type MappedRecord<A extends Record<string, unknown>, B> = {
	[K in keyof A]: B;
};

// # Utility Types / Enums / Constants

export enum ExchangeStatus {
	ACTIVE = 0,
	AMM_PAUSED = 4,
	FILL_PAUSED = 8,
	PAUSED = 127,
}

export class MarketStatus {
	static readonly INITIALIZED = { initialized: {} };
	static readonly ACTIVE = { active: {} };
	static readonly AMM_PAUSED = { ammPaused: {} };
	static readonly FILL_PAUSED = { fillPaused: {} };
	static readonly REDUCE_ONLY = { reduceOnly: {} };
	static readonly SETTLEMENT = { settlement: {} };
	static readonly DELISTED = { delisted: {} };
}

export enum Operation {
	AMM_FILL = 2,
	FILL = 4,
}

export enum InsuranceFundOperation {
	INIT = 1,
	ADD = 2,
	REQUEST_REMOVE = 4,
	REMOVE = 8,
}

export enum UserStatus {
	REDUCE_ONLY = 4,
	ADVANCED_LP = 8,
}

export class SyntheticType {
	static readonly SYNTHETIC = { synthetic: {} };
}

export class SyntheticTier {
	static readonly A = { a: {} };
	static readonly B = { b: {} };
	static readonly C = { c: {} };
	static readonly SPECULATIVE = { speculative: {} };
	static readonly HIGHLY_SPECULATIVE = { highlySpeculative: {} };
	static readonly ISOLATED = { isolated: {} };
}

export class SwapDirection {
	static readonly ADD = { add: {} };
	static readonly REMOVE = { remove: {} };
}

// export class SpotBalanceType {
// 	static readonly DEPOSIT = { deposit: {} };
// 	static readonly BORROW = { borrow: {} };
// }

export class OrderSide {
	static readonly BUY = { buy: {} };
	static readonly SELL = { sell: {} };
}

export class OracleSource {
	static readonly PYTH = { pyth: {} };
	static readonly PYTH_1K = { pyth1K: {} };
	static readonly PYTH_1M = { pyth1M: {} };
	static readonly PYTH_PULL = { pythPull: {} };
	static readonly PYTH_1K_PULL = { pyth1KPull: {} };
	static readonly PYTH_1M_PULL = { pyth1MPull: {} };
	static readonly SWITCHBOARD = { switchboard: {} };
	static readonly QUOTE_ASSET = { quoteAsset: {} };
	static readonly PYTH_STABLE_COIN = { pythStableCoin: {} };
	static readonly PYTH_STABLE_COIN_PULL = { pythStableCoinPull: {} };
	static readonly SWITCHBOARD_ON_DEMAND = { switchboardOnDemand: {} };
}

export class OrderType {
	static readonly LIMIT = { limit: {} };
	static readonly TRIGGER_MARKET = { triggerMarket: {} };
	static readonly TRIGGER_LIMIT = { triggerLimit: {} };
	static readonly MARKET = { market: {} };
}

export declare type MarketTypeStr = 'synthetic';
export class MarketType {
	static readonly SYNTHETIC = { synthetic: {} };
}

export class OrderStatus {
	static readonly INIT = { init: {} };
	static readonly OPEN = { open: {} };
}

export class OrderAction {
	static readonly PLACE = { place: {} };
	static readonly CANCEL = { cancel: {} };
	static readonly EXPIRE = { expire: {} };
	static readonly FILL = { fill: {} };
	static readonly TRIGGER = { trigger: {} };
}

export class OrderActionExplanation {
	static readonly NONE = { none: {} };
	static readonly ORACLE_PRICE_BREACHED_LIMIT_PRICE = {
		oraclePriceBreachedLimitPrice: {},
	};
	static readonly MARKET_ORDER_FILLED_TO_LIMIT_PRICE = {
		marketOrderFilledToLimitPrice: {},
	};
	static readonly ORDER_EXPIRED = {
		orderExpired: {},
	};
	static readonly ORDER_FILLED_WITH_AMM = {
		orderFilledWithAmm: {},
	};
	static readonly ORDER_FILLED_WITH_AMM_JIT = {
		orderFilledWithAmmJit: {},
	};
	static readonly ORDER_FILLED_WITH_AMM_JIT_LP_SPLIT = {
		orderFilledWithAmmJitLpSplit: {},
	};
	static readonly ORDER_FILLED_WITH_LP_JIT = {
		orderFilledWithLpJit: {},
	};
	static readonly ORDER_FILLED_WITH_MATCH = {
		orderFilledWithMatch: {},
	};
	static readonly ORDER_FILLED_WITH_MATCH_JIT = {
		orderFilledWithMatchJit: {},
	};
	static readonly MARKET_EXPIRED = {
		marketExpired: {},
	};
	static readonly RISK_INCREASING_ORDER = {
		riskingIncreasingOrder: {},
	};
	static readonly REDUCE_ONLY_ORDER_INCREASED_POSITION = {
		reduceOnlyOrderIncreasedPosition: {},
	};
	// static readonly DERISK_LP = {
	// 	deriskLp: {},
	// };
}

export class OrderTriggerCondition {
	static readonly ABOVE = { above: {} };
	static readonly BELOW = { below: {} };
	static readonly TRIGGERED_ABOVE = { triggeredAbove: {} }; // above condition has been triggered
	static readonly TRIGGERED_BELOW = { triggeredBelow: {} }; // below condition has been triggered
}

// export class SpotFulfillmentType {
// 	static readonly EXTERNAL = { external: {} };
// 	static readonly MATCH = { match: {} };
// }

// export class SpotFulfillmentStatus {
// 	static readonly ENABLED = { enabled: {} };
// 	static readonly DISABLED = { disabled: {} };
// }

// export class SpotFulfillmentConfigStatus {
// 	static readonly ENABLED = { enabled: {} };
// 	static readonly DISABLED = { disabled: {} };
// }

export class StakeAction {
	static readonly STAKE = { stake: {} };
	static readonly UNSTAKE_REQUEST = { unstakeRequest: {} };
	static readonly UNSTAKE_CANCEL_REQUEST = { unstakeCancelRequest: {} };
	static readonly UNSTAKE = { unstake: {} };
	static readonly UNSTAKE_TRANSFER = { unstakeTransfer: {} };
	static readonly STAKE_TRANSFER = { stakeTransfer: {} };
}

export function isVariant(object: unknown, type: string) {
	return object.hasOwnProperty(type);
}

export function isOneOfVariant(object: unknown, types: string[]) {
	return types.reduce((result, type) => {
		return result || object.hasOwnProperty(type);
	}, false);
}

export function getVariant(object: unknown): string {
	return Object.keys(object)[0];
}

export enum TradeSide {
	None = 0,
	Buy = 1,
	Sell = 2,
}

export type CandleResolution =
	| '1'
	| '5'
	| '15'
	| '60'
	| '240'
	| 'D'
	| 'W'
	| 'M';

export type NewUserRecord = {
	ts: BN;
	userAuthority: PublicKey;
	user: PublicKey;
	subAccountId: number;
	name: number[];
	referrer: PublicKey;
};

export type SpotInterestRecord = {
	ts: BN;
	marketIndex: number;
	depositBalance: BN;
	cumulativeDepositInterest: BN;
	borrowBalance: BN;
	cumulativeBorrowInterest: BN;
	optimalUtilization: number;
	optimalBorrowRate: number;
	maxBorrowRate: number;
};

export type CurveRecord = {
	ts: BN;
	recordId: BN;
	marketIndex: number;
	pegMultiplierBefore: BN;
	baseAssetReserveBefore: BN;
	quoteAssetReserveBefore: BN;
	sqrtKBefore: BN;
	pegMultiplierAfter: BN;
	baseAssetReserveAfter: BN;
	quoteAssetReserveAfter: BN;
	sqrtKAfter: BN;
	baseAssetAmountLong: BN;
	baseAssetAmountShort: BN;
	baseAssetAmountWithAmm: BN;
	totalFee: BN;
	totalFeeMinusDistributions: BN;
	adjustmentCost: BN;
	numberOfUsers: BN;
	oraclePrice: BN;
	fillRecord: BN;
};

export declare type InsuranceFundRecord = {
	ts: BN;
	spotMarketIndex: number;
	perpMarketIndex: number;
	userIfFactor: number;
	totalIfFactor: number;
	vaultAmountBefore: BN;
	insuranceVaultAmountBefore: BN;
	totalIfSharesBefore: BN;
	totalIfSharesAfter: BN;
	amount: BN;
};

export declare type InsuranceFundStakeRecord = {
	ts: BN;
	userAuthority: PublicKey;
	action: StakeAction;
	amount: BN;
	marketIndex: number;
	insuranceVaultAmountBefore: BN;
	ifSharesBefore: BN;
	userIfSharesBefore: BN;
	totalIfSharesBefore: BN;
	ifSharesAfter: BN;
	userIfSharesAfter: BN;
	totalIfSharesAfter: BN;
};

// TODO: add custom LP

export type OrderRecord = {
	ts: BN;
	user: PublicKey;
	order: Order;
};

export type OrderActionRecord = {
	ts: BN;
	action: OrderAction;
	actionExplanation: OrderActionExplanation;
	marketIndex: number;
	marketType: MarketType;
	filler: PublicKey | null;
	fillerReward: BN | null;
	fillRecordId: BN | null;
	baseAssetAmountFilled: BN | null;
	quoteAssetAmountFilled: BN | null;
	takerFee: BN | null;
	makerFee: BN | null;
	referrerReward: number | null;
	quoteAssetAmountSurplus: BN | null;
	spotFulfillmentMethodFee: BN | null;
	taker: PublicKey | null;
	takerOrderId: number | null;
	takerOrderDirection: OrderSide | null;
	takerOrderBaseAssetAmount: BN | null;
	takerOrderCumulativeBaseAssetAmountFilled: BN | null;
	takerOrderCumulativeQuoteAssetAmountFilled: BN | null;
	maker: PublicKey | null;
	makerOrderId: number | null;
	makerOrderDirection: OrderSide | null;
	makerOrderBaseAssetAmount: BN | null;
	makerOrderCumulativeBaseAssetAmountFilled: BN | null;
	makerOrderCumulativeQuoteAssetAmountFilled: BN | null;
	oraclePrice: BN;
};

export type SwapRecord = {
	ts: BN;
	user: PublicKey;
	amountOut: BN;
	amountIn: BN;
	outMarketIndex: number;
	inMarketIndex: number;
	outOraclePrice: BN;
	inOraclePrice: BN;
	fee: BN;
};

export type SpotMarketVaultDepositRecord = {
	ts: BN;
	marketIndex: number;
	depositBalance: BN;
	cumulativeDepositInterestBefore: BN;
	cumulativeDepositInterestAfter: BN;
	depositTokenAmountBefore: BN;
	amount: BN;
};

export type StateAccount = {
	admin: PublicKey;
	exchangeStatus: number;
	whitelistMint: PublicKey;
	discountMint: PublicKey;
	oracleGuardRails: OracleGuardRails;
	numberOfAuthorities: BN;
	numberOfSubAccounts: BN;
	numberOfMarkets: number;
	minAuctionDuration: number;
	defaultMarketOrderTimeInForce: number;
	defaultAuctionDuration: number;
	settlementDuration: number;
	maxNumberOfSubAccounts: number;
	signer: PublicKey;
	signerNonce: number;
	feeStructure: FeeStructure;
	maxInitializeUserFee: number;
};

export type MarketAccount = {
	status: MarketStatus;
	contractType: ContractType;
	contractTier: ContractTier;
	expiryTs: BN;
	expiryPrice: BN;
	marketIndex: number;
	pubkey: PublicKey;
	name: number[];
	amm: AMM;
	numberOfUsersWithBase: number;
	numberOfUsers: number;
	marginRatioInitial: number;
	marginRatioMaintenance: number;
	nextFillRecordId: BN;
	nextFundingRateRecordId: BN;
	nextCurveRecordId: BN;
	pnlPool: PoolBalance;
	liquidatorFee: number;
	ifLiquidationFee: number;
	imfFactor: number;
	unrealizedPnlImfFactor: number;
	unrealizedPnlMaxImbalance: BN;
	unrealizedPnlInitialAssetWeight: number;
	unrealizedPnlMaintenanceAssetWeight: number;
	insuranceClaim: {
		revenueWithdrawSinceLastSettle: BN;
		maxRevenueWithdrawPerPeriod: BN;
		lastRevenueWithdrawTs: BN;
		quoteSettledInsurance: BN;
		quoteMaxInsurance: BN;
	};
	quoteSpotMarketIndex: number;
	feeAdjustment: number;
	pausedOperations: number;
};

export type HistoricalOracleData = {
	lastOraclePrice: BN;
	lastOracleDelay: BN;
	lastOracleConf: BN;
	lastOraclePriceTwap: BN;
	lastOraclePriceTwap5Min: BN;
	lastOraclePriceTwapTs: BN;
};

export type HistoricalIndexData = {
	lastIndexBidPrice: BN;
	lastIndexAskPrice: BN;
	lastIndexPriceTwap: BN;
	lastIndexPriceTwap5Min: BN;
	lastIndexPriceTwapTs: BN;
};

export type PoolBalance = {
	balance: BN;
	marketIndex: number;
};

export type AMM = {
	baseAssetReserve: BN;
	sqrtK: BN;
	cumulativeFundingRate: BN;
	lastFundingRate: BN;
	lastFundingRateTs: BN;
	lastMarkPriceTwap: BN;
	lastMarkPriceTwap5Min: BN;
	lastMarkPriceTwapTs: BN;
	lastTradeTs: BN;

	oracle: PublicKey;
	oracleSource: OracleSource;
	historicalOracleData: HistoricalOracleData;

	lastOracleReservePriceSpreadPct: BN;
	lastOracleConfPct: BN;

	fundingPeriod: BN;
	quoteAssetReserve: BN;
	pegMultiplier: BN;
	cumulativeFundingRateLong: BN;
	cumulativeFundingRateShort: BN;
	last24HAvgFundingRate: BN;
	lastFundingRateShort: BN;
	lastFundingRateLong: BN;

	totalLiquidationFee: BN;
	totalFeeMinusDistributions: BN;
	totalFeeWithdrawn: BN;
	totalFee: BN;
	totalFeeEarnedPerLp: BN;
	userLpShares: BN;
	baseAssetAmountWithUnsettledLp: BN;
	orderStepSize: BN;
	orderTickSize: BN;
	maxFillReserveFraction: number;
	maxSlippageRatio: number;
	baseSpread: number;
	curveUpdateIntensity: number;
	baseAssetAmountWithAmm: BN;
	baseAssetAmountLong: BN;
	baseAssetAmountShort: BN;
	quoteAssetAmount: BN;
	terminalQuoteAssetReserve: BN;
	concentrationCoef: BN;
	feePool: PoolBalance;
	totalExchangeFee: BN;
	totalMmFee: BN;
	netRevenueSinceLastFunding: BN;
	lastUpdateSlot: BN;
	lastOracleNormalisedPrice: BN;
	lastOracleValid: boolean;
	lastBidPriceTwap: BN;
	lastAskPriceTwap: BN;
	longSpread: number;
	shortSpread: number;
	maxSpread: number;

	baseAssetAmountPerLp: BN;
	quoteAssetAmountPerLp: BN;
	targetBaseAssetAmountPerLp: number;

	ammJitIntensity: number;
	maxOpenInterest: BN;
	maxBaseAssetReserve: BN;
	minBaseAssetReserve: BN;
	totalSocialLoss: BN;

	quoteBreakEvenAmountLong: BN;
	quoteBreakEvenAmountShort: BN;
	quoteEntryAmountLong: BN;
	quoteEntryAmountShort: BN;

	markStd: BN;
	oracleStd: BN;
	longIntensityCount: number;
	longIntensityVolume: BN;
	shortIntensityCount: number;
	shortIntensityVolume: BN;
	volume24H: BN;
	minOrderSize: BN;
	maxPositionSize: BN;

	bidBaseAssetReserve: BN;
	bidQuoteAssetReserve: BN;
	askBaseAssetReserve: BN;
	askQuoteAssetReserve: BN;

	perLpBase: number; // i8
	netUnsettledFundingPnl: BN;
	quoteAssetAmountWithUnsettledLp: BN;
	referencePriceOffset: number;
};

// # User Account Types
export type Position = {
	marketIndex: number;
	openOrders: number;
	openBids: BN;
	openAsks: BN;
	remainderBaseAssetAmount: number;
};

export type UserStatsAccount = {
	numberOfSubAccounts: number;
	numberOfSubAccountsCreated: number;
	makerVolume30D: BN;
	takerVolume30D: BN;
	fillerVolume30D: BN;
	lastMakerVolume30DTs: BN;
	lastTakerVolume30DTs: BN;
	lastFillerVolume30DTs: BN;
	fees: {
		totalFeePaid: BN;
		totalFeeRebate: BN;
		totalTokenDiscount: BN;
		totalRefereeDiscount: BN;
		totalReferrerReward: BN;
		current_epoch_referrer_reward: BN;
	};
	referrer: PublicKey;
	isReferrer: boolean;
	authority: PublicKey;
	ifStakedQuoteAssetAmount: BN;

	ifStakedGovTokenAmount: BN;
};

export type UserAccount = {
	authority: PublicKey;
	delegate: PublicKey;
	name: number[];
	subAccountId: number;
	positions: Position[];
	orders: Order[];
	status: number;
	nextOrderId: number;
	cumulativeSpotFees: BN;
	lastActiveSlot: BN;
	idle: boolean;
	openOrders: number;
	hasOpenOrder: boolean;
	openAuctions: number;
	hasOpenAuction: boolean;
};

export type Order = {
	status: OrderStatus;
	orderType: OrderType;
	marketType: MarketType;
	slot: BN;
	orderId: number;
	userOrderId: number;
	marketIndex: number;
	price: BN;
	baseAssetAmount: BN;
	quoteAssetAmount: BN;
	baseAssetAmountFilled: BN;
	quoteAssetAmountFilled: BN;
	side: OrderSide;
	reduceOnly: boolean;
	triggerPrice: BN;
	triggerCondition: OrderTriggerCondition;
	existingPositionDirection: PositionDirection;
	postOnly: boolean;
	immediateOrCancel: boolean;
	oraclePriceOffset: number;
	auctionDuration: number;
	auctionStartPrice: BN;
	auctionEndPrice: BN;
	maxTs: BN;
};

export type OrderParams = {
	orderType: OrderType;
	marketType: MarketType;
	userOrderId: number;
	side: OrderSide;
	baseAssetAmount: BN;
	price: BN;
	marketIndex: number;
	reduceOnly: boolean;
	postOnly: PostOnlyParams;
	immediateOrCancel: boolean;
	triggerPrice: BN | null;
	triggerCondition: OrderTriggerCondition;
	oraclePriceOffset: number | null;
	auctionDuration: number | null;
	maxTs: BN | null;
	auctionStartPrice: BN | null;
	auctionEndPrice: BN | null;
};

export class PostOnlyParams {
	static readonly NONE = { none: {} };
	static readonly MUST_POST_ONLY = { mustPostOnly: {} }; // Tx fails if order can't be post only
	static readonly TRY_POST_ONLY = { tryPostOnly: {} }; // Tx succeeds and order not placed if can't be post only
	static readonly SLIDE = { slide: {} }; // Modify price to be post only if can't be post only
}

export type NecessaryOrderParams = {
	orderType: OrderType;
	marketIndex: number;
	baseAssetAmount: BN;
	side: OrderSide;
};

export type OptionalOrderParams = {
	[Property in keyof OrderParams]?: OrderParams[Property];
} & NecessaryOrderParams;

export type ModifyOrderParams = {
	[Property in keyof OrderParams]?: OrderParams[Property] | null;
} & { policy?: ModifyOrderPolicy };

export class ModifyOrderPolicy {
	static readonly MUST_MODIFY = { mustModify: {} };
	static readonly TRY_MODIFY = { tryModify: {} };
}

export const DefaultOrderParams: OrderParams = {
	orderType: OrderType.MARKET,
	marketType: MarketType.SYNTHETIC,
	userOrderId: 0,
	side: OrderSide.BUY,
	baseAssetAmount: ZERO,
	price: ZERO,
	marketIndex: 0,
	reduceOnly: false,
	postOnly: PostOnlyParams.NONE,
	immediateOrCancel: false,
	triggerPrice: null,
	triggerCondition: OrderTriggerCondition.ABOVE,
	oraclePriceOffset: null,
	auctionDuration: null,
	maxTs: null,
	auctionStartPrice: null,
	auctionEndPrice: null,
};

export type SwiftServerMessage = {
	slot: BN;
	swiftOrderSignature: Uint8Array;
};

export type SwiftOrderParamsMessage = {
	swiftOrderParams: OptionalOrderParams;
	expectedOrderId: number;
	takeProfitOrderParams: SwiftTriggerOrderParams | null;
	stopLossOrderParams: SwiftTriggerOrderParams | null;
};

export type SwiftTriggerOrderParams = {
	triggerPrice: BN;
	baseAssetAmount: BN;
};

export type MakerInfo = {
	maker: PublicKey;
	makerStats: PublicKey;
	makerUserAccount: UserAccount;
	order?: Order;
};

export type TakerInfo = {
	taker: PublicKey;
	takerStats: PublicKey;
	takerUserAccount: UserAccount;
	order: Order;
};

export type ReferrerInfo = {
	referrer: PublicKey;
	referrerStats: PublicKey;
};

export enum PlaceAndTakeOrderSuccessCondition {
	PartialFill = 1,
	FullFill = 2,
}

type ExactType<T> = Pick<T, keyof T>;

export type BaseTxParams = ExactType<{
	computeUnits?: number;
	computeUnitsPrice?: number;
}>;

export type ProcessingTxParams = {
	useSimulatedComputeUnits?: boolean;
	computeUnitsBufferMultiplier?: number;
	useSimulatedComputeUnitsForCUPriceCalculation?: boolean;
	getCUPriceFromComputeUnits?: (computeUnits: number) => number;
	lowerBoundCu?: number;
};

export type TxParams = BaseTxParams & ProcessingTxParams;

export class SwapReduceOnly {
	static readonly In = { in: {} };
	static readonly Out = { out: {} };
}

// # Misc Types
export interface IWallet {
	signTransaction(tx: Transaction): Promise<Transaction>;
	signAllTransactions(txs: Transaction[]): Promise<Transaction[]>;
	publicKey: PublicKey;
	payer?: Keypair;
}
export interface IVersionedWallet {
	signVersionedTransaction(
		tx: VersionedTransaction
	): Promise<VersionedTransaction>;
	signAllVersionedTransactions(
		txs: VersionedTransaction[]
	): Promise<VersionedTransaction[]>;
	publicKey: PublicKey;
	payer?: Keypair;
}

export type FeeStructure = {
	feeTiers: FeeTier[];
	fillerRewardStructure: OrderFillerRewardStructure;
	flatFillerFee: BN;
	referrerRewardEpochUpperBound: BN;
};

export type FeeTier = {
	feeNumerator: number;
	feeDenominator: number;
	makerRebateNumerator: number;
	makerRebateDenominator: number;
	referrerRewardNumerator: number;
	referrerRewardDenominator: number;
	refereeFeeNumerator: number;
	refereeFeeDenominator: number;
};

export type OrderFillerRewardStructure = {
	rewardNumerator: BN;
	rewardDenominator: BN;
	timeBasedRewardLowerBound: BN;
};

export type OracleGuardRails = {
	priceDivergence: {
		markOraclePercentDivergence: BN;
		oracleTwap5MinPercentDivergence: BN;
	};
	validity: {
		slotsBeforeStaleForAmm: BN;
		slotsBeforeStaleForMargin: BN;
		confidenceIntervalMaxSize: BN;
		tooVolatileRatio: BN;
	};
};

export type InsuranceFundStake = {
	costBasis: BN;

	marketIndex: number;
	authority: PublicKey;

	ifShares: BN;
	ifBase: BN;

	lastWithdrawRequestShares: BN;
	lastWithdrawRequestValue: BN;
	lastWithdrawRequestTs: BN;
};

export type ReferrerNameAccount = {
	name: number[];
	user: PublicKey;
	authority: PublicKey;
	userStats: PublicKey;
};

export type MarketExtendedInfo = {
	marketIndex: number;
	/**
	 * Min order size measured in base asset, using base precision
	 */
	minOrderSize: BN;
	/**
	 * Max insurance available, measured in quote asset, using quote preicision
	 */
	availableInsurance: BN;
	/**
	 * Pnl pool available, this is measured in quote asset, using quote precision.
	 * Should be generated by using getTokenAmount and passing in the scaled balance of the base asset + quote spot account
	 */
	pnlPoolValue: BN;
	syntheticTier: SyntheticTier;
};

export type HealthComponents = {
	positions: HealthComponent[];
};

export type HealthComponent = {
	marketIndex: number;
	size: BN;
	value: BN;
	weight: BN;
	weightedValue: BN;
};

export interface DriftClientMetricsEvents {
	txSigned: SignedTxData[];
	preTxSigned: void;
}

export type SignedTxData = {
	txSig: string;
	signedTx: Transaction | VersionedTransaction;
	lastValidBlockHeight?: number;
	blockHash: string;
};
