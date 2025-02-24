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
	LIQ_PAUSED = 8,
	PAUSED = 127,
}

export class BalanceType {
	static readonly DEPOSIT = { deposit: {} };
	static readonly BORROW = { borrow: {} };
}

export class SynthMarketStatus {
	static readonly INITIALIZED = { initialized: {} };
	static readonly ACTIVE = { active: {} };
	static readonly AMM_PAUSED = { ammPaused: {} };
	static readonly WITHDRAW_PAUSED = { withdrawPaused: {} };
	static readonly REDUCE_ONLY = { reduceOnly: {} };
	static readonly SETTLEMENT = { settlement: {} };
	static readonly DELISTED = { delisted: {} };
}

export enum SynthOperation {
	CREATE = 1,
	DEPOSIT = 2,
	WITHDRAW = 4,
	LEND = 8,
	TRANSFER = 16,
	DELETE = 32,
	LIQUIDATION = 64,
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

export class Tier {
	static readonly A = { a: {} };
	static readonly B = { b: {} };
	static readonly C = { c: {} };
	static readonly SPECULATIVE = { speculative: {} };
	static readonly HIGHLY_SPECULATIVE = { highlySpeculative: {} };
	static readonly ISOLATED = { isolated: {} };
}

export class OracleSource {
	static readonly PYTH = { pyth: {} };
	static readonly PYTH_1K = { pyth1K: {} };
	static readonly PYTH_1M = { pyth1M: {} };
	static readonly PYTH_PULL = { pythPull: {} };
	static readonly PYTH_1K_PULL = { pyth1KPull: {} };
	static readonly PYTH_1M_PULL = { pyth1MPull: {} };
	static readonly QUOTE_ASSET = { quoteAsset: {} };
	static readonly PYTH_STABLE_COIN = { pythStableCoin: {} };
	static readonly PYTH_STABLE_COIN_PULL = { pythStableCoinPull: {} };
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

export declare type InsuranceFundRecord = {
	ts: BN;
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
	insuranceVaultAmountBefore: BN;
	ifSharesBefore: BN;
	userIfSharesBefore: BN;
	totalIfSharesBefore: BN;
	ifSharesAfter: BN;
	userIfSharesAfter: BN;
	totalIfSharesAfter: BN;
};

export type StateAccount = {
	admin: PublicKey;
	signer: PublicKey;
	signerNonce: number;

	oracleGuardRails: OracleGuardRails;
	emergencyOracles: any[];

	exchangeStatus: number;
	numberOfMarkets: number;
	numberOfVaults: number;

	insuranceFund: PublicKey;
	totalDebtCeiling: BN;

	maxInitializeUserFee: number;
	numberOfAuthorities: BN;
	numberOfSubAccounts: BN;
	maxNumberOfSubAccounts: number;

	liquidationMarginBufferRatio: number;
	liquidationDuration: number;
	initialPctToLiquidate: number;
};

// TODO: not all these need to be BN
export type InsuranceFundAccount = {
	pubkey: PublicKey;
	authority: PublicKey;
	vault: PublicKey;
	mint: PublicKey;
	totalShares: BN;
	userShares: BN;
	sharesBase: BN;
	unstakingPeriod: BN;
	lastRevenueSettleTs: BN;
	revenueSettlePeriod: BN;
	totalFactor: BN;
	userFactor: BN;
	maxInsurance: BN;
	pausedOperations: number;
	tokenProgram: number;
};

export type SyntheticParams = {
	token_mint: PublicKey;
	vault: PublicKey;
	tier: SynthTier;
	balance: BN;
	token_twap: BN;
	max_position_size: BN;
};

export type CollateralParams = {
	// symbol: String;
	token_mint: PublicKey;
	vault: PublicKey;
	oracle: PublicKey;
	oracleSource: OracleSource;
	oracle_frozen: boolean;
	balance: BN;
	pool_delta_balance: BN;
	token_twap: BN;
	margin_ratio_initial: BN;
	margin_ratio_maintenance: BN;
	max_token_deposits: BN;
	max_token_borrows_fraction: BN;
	withdraw_guard_threshold: BN;
};

export type MarketAccount = {
	pubkey: PublicKey;
	decimals: number;
	amm: AMM;
	synthetic: SyntheticParams;
	collateral: CollateralParams;
	marketIndex: number;
	name: number[];
	status: SynthMarketStatus;
	pausedOperations: number;
	numberOfUsers: number;
	liquidation_penalty: number;
	liquidatorFee: number;
	ifLiquidationFee: number;
	imfFactor: number;
	debtCeiling: BN;
	debtFloor: number;
	insuranceClaim: {
		revenueWithdrawSinceLastSettle: BN;
		maxRevenueWithdrawPerPeriod: BN;
		lastRevenueWithdrawTs: BN;
		quoteSettledInsurance: BN;
		quoteMaxInsurance: BN;
	};
	expiryTs: BN;
	expiryPrice: BN;
};

export type AMMRewardInfo = {
	mint: PublicKey;
	vault: PublicKey;
	authority: PublicKey;
	emissions_per_second_x64: BN;
	growth_global_x64: BN;
};

export type AMM = {
	token_mint_a: PublicKey;
	token_mint_b: PublicKey;
	token_vault_a: PublicKey;
	token_vault_b: PublicKey;
	// token_mint_lp: PublicKey;
	tick_spacing: number;
	tick_spacing_seed;
	tick_current_index;

	oracle: PublicKey;
	oracleSource: OracleSource;
	historical_oracle_data: HistoricalOracleData;
	last_oracle_normalised_price: BN;
	last_oracle_price_spread_pct: BN;
	last_price_twap: BN;
	last_oracle_conf_pct: BN;
	oracle_std: BN;
	last_price_twap_ts: BN;
	last_oracle_valid: boolean;

	liquidity: BN;
	sqrt_price: BN;
	fee_rate: BN;
	protocol_fee_rate: BN;
	fee_growth_global_a: BN;
	fee_growth_global_b: BN;
	protocol_fee_owed_a: BN;
	protocol_fee_owed_b: BN;
	max_allowed_slippage_bps: BN;
	max_allowed_variance_bps: BN;
	reward_last_updated_timestamp: BN;
	reward_infos: AMMRewardInfo[];
	last_update_slot: BN;
};

export type HistoricalOracleData = {
	lastOraclePrice: BN;
	lastOracleDelay: BN;
	lastOracleConf: BN;
	lastOraclePriceTwap: BN;
	lastOraclePriceTwap5Min: BN;
	lastOraclePriceTwapTs: BN;
};

export type PoolBalance = {
	scaledBalance: BN;
	marketIndex: number;
};

export type UserStatsAccount = {
	numberOfSubAccounts: number;
	numberOfSubAccountsCreated: number;
	makerVolume30D: BN;
	takerVolume30D: BN;
	lastMakerVolume30DTs: BN;
	lastTakerVolume30DTs: BN;
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
	// positions: Position[];
	status: number;
	nextLiquidationId: number;
	maxMarginRatio: number;
	totalDeposits: BN;
	totalWithdraws: BN;
	liquidationMarginFreed: BN;
	lastActiveSlot: BN;
	idle: boolean;
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
	// fillerRewardStructure: OrderFillerRewardStructure;
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
	Tier: Tier;
};

export type HealthComponents = {
	deposits: HealthComponent[];
	borrows: HealthComponent[];
	vaultPositions: HealthComponent[];
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
