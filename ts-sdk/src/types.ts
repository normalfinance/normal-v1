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
	DEPOSIT_PAUSED = 1,
	WITHDRAW_PAUSED = 2,
	AMM_PAUSED = 4,
	FILL_PAUSED = 8,
	LIQ_PAUSED = 16,
	FUNDING_PAUSED = 32,
	SETTLE_PNL_PAUSED = 64,
	PAUSED = 127,
}

export class MarketStatus {
	static readonly INITIALIZED = { initialized: {} };
	static readonly ACTIVE = { active: {} };
	static readonly FUNDING_PAUSED = { fundingPaused: {} };
	static readonly AMM_PAUSED = { ammPaused: {} };
	static readonly FILL_PAUSED = { fillPaused: {} };
	static readonly WITHDRAW_PAUSED = { withdrawPaused: {} };
	static readonly REDUCE_ONLY = { reduceOnly: {} };
	static readonly SETTLEMENT = { settlement: {} };
	static readonly DELISTED = { delisted: {} };
}

export enum VaultOperation {
	UPDATE_FUNDING = 1,
	AMM_FILL = 2,
	FILL = 4,
	SETTLE_PNL = 8,
	SETTLE_PNL_WITH_POSITION = 16,
	LIQUIDATION = 32,
}

export enum InsuranceFundOperation {
	INIT = 1,
	ADD = 2,
	REQUEST_REMOVE = 4,
	REMOVE = 8,
}

export enum UserStatus {
	BEING_LIQUIDATED = 1,
	BANKRUPT = 2,
	REDUCE_ONLY = 4,
}

export class ContractType {
	static readonly PERPETUAL = { perpetual: {} };
	static readonly FUTURE = { future: {} };
	static readonly PREDICTION = { prediction: {} };
}

export class ContractTier {
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

export class SpotBalanceType {
	static readonly DEPOSIT = { deposit: {} };
	static readonly BORROW = { borrow: {} };
}

export class DepositDirection {
	static readonly DEPOSIT = { deposit: {} };
	static readonly WITHDRAW = { withdraw: {} };
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

export declare type MarketTypeStr = 'perp' | 'spot';
export class MarketType {
	static readonly SPOT = { spot: {} };
	static readonly PERP = { perp: {} };
}

export class DepositExplanation {
	static readonly NONE = { none: {} };
	static readonly TRANSFER = { transfer: {} };
	static readonly BORROW = { borrow: {} };
	static readonly REPAY_BORROW = { repayBorrow: {} };
}

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

export type DepositRecord = {
	ts: BN;
	userAuthority: PublicKey;
	user: PublicKey;
	direction: {
		deposit?: any;
		withdraw?: any;
	};
	marketIndex: number;
	amount: BN;
	oraclePrice: BN;
	marketDepositBalance: BN;
	marketWithdrawBalance: BN;
	marketCumulativeDepositInterest: BN;
	marketCumulativeBorrowInterest: BN;
	totalDepositsAfter: BN;
	totalWithdrawsAfter: BN;
	depositRecordId: BN;
	explanation: DepositExplanation;
	transferUser?: PublicKey;
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

export type LPRecord = {
	ts: BN;
	user: PublicKey;
	action: LPAction;
	nShares: BN;
	marketIndex: number;
	deltaBaseAssetAmount: BN;
	deltaQuoteAssetAmount: BN;
	pnl: BN;
};

export class LPAction {
	static readonly ADD_LIQUIDITY = { addLiquidity: {} };
	static readonly REMOVE_LIQUIDITY = { removeLiquidity: {} };
	static readonly SETTLE_LIQUIDITY = { settleLiquidity: {} };
	static readonly REMOVE_LIQUIDITY_DERISK = { removeLiquidityDerisk: {} };
}

export type LiquidationRecord = {
	ts: BN;
	user: PublicKey;
	liquidator: PublicKey;
	liquidationType: LiquidationType;
	marginRequirement: BN;
	totalCollateral: BN;
	marginFreed: BN;
	liquidationId: number;
	bankrupt: boolean;
	canceledOrderIds: BN[];
	liquidatePerp: LiquidatePerpRecord;
	liquidateSpot: LiquidateSpotRecord;
	liquidateBorrowForPerpPnl: LiquidateBorrowForPerpPnlRecord;
	liquidatePerpPnlForDeposit: LiquidatePerpPnlForDepositRecord;
	perpBankruptcy: PerpBankruptcyRecord;
	spotBankruptcy: SpotBankruptcyRecord;
};

export class LiquidationType {
	static readonly LIQUIDATE_PERP = { liquidatePerp: {} };
	static readonly LIQUIDATE_BORROW_FOR_PERP_PNL = {
		liquidateBorrowForPerpPnl: {},
	};
	static readonly LIQUIDATE_PERP_PNL_FOR_DEPOSIT = {
		liquidatePerpPnlForDeposit: {},
	};
	static readonly PERP_BANKRUPTCY = {
		perpBankruptcy: {},
	};
	static readonly SPOT_BANKRUPTCY = {
		spotBankruptcy: {},
	};
	static readonly LIQUIDATE_SPOT = {
		liquidateSpot: {},
	};
}

export type LiquidatePerpRecord = {
	marketIndex: number;
	oraclePrice: BN;
	baseAssetAmount: BN;
	quoteAssetAmount: BN;
	lpShares: BN;
	userOrderId: BN;
	liquidatorOrderId: BN;
	fillRecordId: BN;
	liquidatorFee: BN;
	ifFee: BN;
};

export type PerpBankruptcyRecord = {
	marketIndex: number;
	ifPayment: BN;
	clawbackUser: PublicKey | null;
	clawbackUserPayment: BN | null;
	cumulativeFundingRateDelta: BN;
	// borrowAmount: BN;
	// cumulativeDepositInterestDelta: BN;
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
	numberOfSpotMarkets: number;
	minPerpAuctionDuration: number;
	defaultMarketOrderTimeInForce: number;
	defaultSpotAuctionDuration: number;
	liquidationMarginBufferRatio: number;
	settlementDuration: number;
	maxNumberOfSubAccounts: number;
	signer: PublicKey;
	signerNonce: number;
	srmVault: PublicKey;
	perpFeeStructure: FeeStructure;
	spotFeeStructure: FeeStructure;
	lpCooldownTime: BN;
	initialPctToLiquidate: number;
	liquidationDuration: number;
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

	fuelBoostTaker: number;
	fuelBoostMaker: number;
	fuelBoostPosition: number;
};

export type VaultAccount = {
	vaultIndex: number;
}

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

export type SpotMarketAccount = {
	status: MarketStatus;
	assetTier: AssetTier;
	name: number[];

	marketIndex: number;
	pubkey: PublicKey;
	mint: PublicKey;
	vault: PublicKey;

	oracle: PublicKey;
	oracleSource: OracleSource;
	historicalOracleData: HistoricalOracleData;
	historicalIndexData: HistoricalIndexData;

	insuranceFund: {
		vault: PublicKey;
		totalShares: BN;
		userShares: BN;
		sharesBase: BN;
		unstakingPeriod: BN;
		lastRevenueSettleTs: BN;
		revenueSettlePeriod: BN;
		totalFactor: number;
		userFactor: number;
	};

	revenuePool: PoolBalance;

	ifLiquidationFee: number;

	decimals: number;
	optimalUtilization: number;
	optimalBorrowRate: number;
	maxBorrowRate: number;
	cumulativeDepositInterest: BN;
	cumulativeBorrowInterest: BN;
	totalSocialLoss: BN;
	totalQuoteSocialLoss: BN;
	depositBalance: BN;
	borrowBalance: BN;
	maxTokenDeposits: BN;

	lastInterestTs: BN;
	lastTwapTs: BN;
	initialAssetWeight: number;
	maintenanceAssetWeight: number;
	initialLiabilityWeight: number;
	maintenanceLiabilityWeight: number;
	liquidatorFee: number;
	imfFactor: number;
	scaleInitialAssetWeightStart: BN;

	withdrawGuardThreshold: BN;
	depositTokenTwap: BN;
	borrowTokenTwap: BN;
	utilizationTwap: BN;
	nextDepositRecordId: BN;

	orderStepSize: BN;
	orderTickSize: BN;
	minOrderSize: BN;
	maxPositionSize: BN;
	nextFillRecordId: BN;
	spotFeePool: PoolBalance;
	totalSpotFee: BN;
	totalSwapFee: BN;

	flashLoanAmount: BN;
	flashLoanInitialTokenAmount: BN;

	ordersEnabled: boolean;

	pausedOperations: number;

	ifPausedOperations: number;

	maxTokenBorrowsFraction: number;
	minBorrowRate: number;

	fuelBoostDeposits: number;
	fuelBoostBorrows: number;
	fuelBoostTaker: number;
	fuelBoostMaker: number;
	fuelBoostInsurance: number;

	tokenProgram: number;
};

export type PoolBalance = {
	scaledBalance: BN;
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
export type VaultPosition = {
	baseAssetAmount: BN;
	lastCumulativeFundingRate: BN;
	marketIndex: number;
	quoteAssetAmount: BN;
	quoteEntryAmount: BN;
	quoteBreakEvenAmount: BN;
	openOrders: number;
	openBids: BN;
	openAsks: BN;
	settledPnl: BN;
	lpShares: BN;
	remainderBaseAssetAmount: number;
	lastBaseAssetAmountPerLp: BN;
	lastQuoteAssetAmountPerLp: BN;
	perLpBase: number;
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

	lastFuelIfBonusUpdateTs: number; // u32 onchain

	fuelInsurance: number;
	fuelDeposits: number;
	fuelBorrows: number;
	fuelPositions: number;
	fuelTaker: number;
	fuelMaker: number;

	ifStakedGovTokenAmount: BN;
};

export type UserAccount = {
	authority: PublicKey;
	delegate: PublicKey;
	name: number[];
	subAccountId: number;
	vaultPositions: VaultPosition[];
	status: number;
	nextLiquidationId: number;
	maxMarginRatio: number;
	totalDeposits: BN;
	totalWithdraws: BN;
	cumulativeSpotFees: BN;
	liquidationMarginFreed: BN;
	lastActiveSlot: BN;
	idle: boolean;
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

export type MarginCategory = 'Initial' | 'Maintenance';

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

export type PerpMarketExtendedInfo = {
	marketIndex: number;
	/**
	 * Min order size measured in base asset, using base precision
	 */
	minOrderSize: BN;
	/**
	 * Margin maintenance percentage, using margin precision (1e4)
	 */
	marginMaintenance: number;
	/**
	 * Max insurance available, measured in quote asset, using quote preicision
	 */
	availableInsurance: BN;
	/**
	 * Pnl pool available, this is measured in quote asset, using quote precision.
	 * Should be generated by using getTokenAmount and passing in the scaled balance of the base asset + quote spot account
	 */
	pnlPoolValue: BN;
	contractTier: ContractTier;
};

export type HealthComponents = {
	deposits: HealthComponent[];
	borrows: HealthComponent[];
	perpPositions: HealthComponent[];
	perpPnl: HealthComponent[];
};

export type HealthComponent = {
	marketIndex: number;
	size: BN;
	value: BN;
	weight: BN;
	weightedValue: BN;
};

export interface NormalClientMetricsEvents {
	txSigned: SignedTxData[];
	preTxSigned: void;
}

export type SignedTxData = {
	txSig: string;
	signedTx: Transaction | VersionedTransaction;
	lastValidBlockHeight?: number;
	blockHash: string;
};
