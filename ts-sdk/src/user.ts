import { PublicKey } from '@solana/web3.js';
import { EventEmitter } from 'events';
import StrictEventEmitter from 'strict-event-emitter-types';
import { NormalClient } from './normalClient';
import {
	HealthComponent,
	HealthComponents,
	isVariant,
	MarginCategory,
	MarketAccount,
	VaultPosition,
	UserAccount,
	UserStatus,
	UserStatsAccount,
} from './types';
import { calculateEntryPrice, positionIsAvailable } from './math/position';
import {
	AMM_RESERVE_PRECISION,
	AMM_RESERVE_PRECISION_EXP,
	AMM_TO_QUOTE_PRECISION_RATIO,
	BASE_PRECISION,
	BN_MAX,
	DUST_POSITION_SIZE,
	FIVE_MINUTE,
	MARGIN_PRECISION,
	ONE,
	OPEN_ORDER_MARGIN_REQUIREMENT,
	PRICE_PRECISION,
	QUOTE_PRECISION,
	QUOTE_PRECISION_EXP,
	QUOTE_SPOT_MARKET_INDEX,
	SPOT_MARKET_WEIGHT_PRECISION,
	GOV_SPOT_MARKET_INDEX,
	TEN,
	TEN_THOUSAND,
	TWO,
	ZERO,
	FUEL_START_TS,
} from './constants/numericConstants';
import {
	DataAndSlot,
	UserAccountEvents,
	UserAccountSubscriber,
} from './accounts/types';
import {
	BigNum,
	BN,
	calculateBaseAssetValue,
	calculateMarketMarginRatio,
	calculatePerpLiabilityValue,
	calculatePositionFundingPNL,
	calculatePositionPNL,
	calculateReservePrice,
	calculateSpotMarketMarginRatio,
	calculateUnrealizedAssetWeight,
	calculateWorstCasePerpLiabilityValue,
	divCeil,
	getBalance,
	getSignedTokenAmount,
	getStrictTokenValue,
	getTokenValue,
	getUser30dRollingVolumeEstimate,
	MarketType,
	PositionDirection,
	sigNum,
	SpotBalanceType,
	SpotMarketAccount,
	standardizeBaseAssetAmount,
} from '.';
import {
	calculateAssetWeight,
	calculateLiabilityWeight,
	calculateWithdrawLimit,
	getTokenAmount,
} from './math/spotBalance';
import { calculateMarketOpenBidAsk } from './math/amm';
import {
	calculateBaseAssetValueWithOracle,
	calculateCollateralDepositRequiredForTrade,
	calculateMarginUSDCRequiredForTrade,
	calculateWorstCaseBaseAssetAmount,
} from './math/margin';
import { OraclePriceData } from './oracles/types';
import { UserConfig } from './userConfig';
import { PollingUserAccountSubscriber } from './accounts/pollingUserAccountSubscriber';
import { WebSocketUserAccountSubscriber } from './accounts/webSocketUserAccountSubscriber';
import {
	calculateWeightedTokenValue,
	getWorstCaseTokenAmounts,
	isSpotPositionAvailable,
} from './math/spotPosition';
import { calculateLiveOracleTwap } from './math/oracles';
import { getPerpMarketTierNumber, getSpotMarketTierNumber } from './math/tiers';
import { StrictOraclePrice } from './oracles/strictOraclePrice';

export class User {
	normalClient: NormalClient;
	userAccountPublicKey: PublicKey;
	accountSubscriber: UserAccountSubscriber;
	_isSubscribed = false;
	eventEmitter: StrictEventEmitter<EventEmitter, UserAccountEvents>;

	public get isSubscribed() {
		return this._isSubscribed && this.accountSubscriber.isSubscribed;
	}

	public set isSubscribed(val: boolean) {
		this._isSubscribed = val;
	}

	public constructor(config: UserConfig) {
		this.normalClient = config.normalClient;
		this.userAccountPublicKey = config.userAccountPublicKey;
		if (config.accountSubscription?.type === 'polling') {
			this.accountSubscriber = new PollingUserAccountSubscriber(
				config.normalClient.connection,
				config.userAccountPublicKey,
				config.accountSubscription.accountLoader,
				this.normalClient.program.account.user.coder.accounts.decodeUnchecked.bind(
					this.normalClient.program.account.user.coder.accounts
				)
			);
		} else if (config.accountSubscription?.type === 'custom') {
			this.accountSubscriber = config.accountSubscription.userAccountSubscriber;
		} else {
			this.accountSubscriber = new WebSocketUserAccountSubscriber(
				config.normalClient.program,
				config.userAccountPublicKey,
				{
					resubTimeoutMs: config.accountSubscription?.resubTimeoutMs,
					logResubMessages: config.accountSubscription?.logResubMessages,
				},
				config.accountSubscription?.commitment
			);
		}
		this.eventEmitter = this.accountSubscriber.eventEmitter;
	}

	/**
	 * Subscribe to User state accounts
	 * @returns SusbcriptionSuccess result
	 */
	public async subscribe(userAccount?: UserAccount): Promise<boolean> {
		this.isSubscribed = await this.accountSubscriber.subscribe(userAccount);
		return this.isSubscribed;
	}

	/**
	 *	Forces the accountSubscriber to fetch account updates from rpc
	 */
	public async fetchAccounts(): Promise<void> {
		await this.accountSubscriber.fetch();
	}

	public async unsubscribe(): Promise<void> {
		await this.accountSubscriber.unsubscribe();
		this.isSubscribed = false;
	}

	public getUserAccount(): UserAccount {
		return this.accountSubscriber.getUserAccountAndSlot().data;
	}

	public async forceGetUserAccount(): Promise<UserAccount> {
		await this.fetchAccounts();
		return this.accountSubscriber.getUserAccountAndSlot().data;
	}

	public getUserAccountAndSlot(): DataAndSlot<UserAccount> | undefined {
		return this.accountSubscriber.getUserAccountAndSlot();
	}

	public getVaultPositionForUserAccount(
		userAccount: UserAccount,
		marketIndex: number
	): VaultPosition | undefined {
		return this.getActiveVaultPositionsForUserAccount(userAccount).find(
			(position) => position.marketIndex === marketIndex
		);
	}

	/**
	 * Gets the user's current position for a given perp market. If the user has no position returns undefined
	 * @param marketIndex
	 * @returns userVaultPosition
	 */
	public getVaultPosition(marketIndex: number): VaultPosition | undefined {
		const userAccount = this.getUserAccount();
		return this.getVaultPositionForUserAccount(userAccount, marketIndex);
	}

	public getVaultPositionAndSlot(
		marketIndex: number
	): DataAndSlot<VaultPosition | undefined> {
		const userAccount = this.getUserAccountAndSlot();
		const perpPosition = this.getVaultPositionForUserAccount(
			userAccount.data,
			marketIndex
		);
		return {
			data: perpPosition,
			slot: userAccount.slot,
		};
	}

	/**
	 * Returns the token amount for a given market. The spot market precision is based on the token mint decimals.
	 * Positive if it is a deposit, negative if it is a borrow.
	 *
	 * @param marketIndex
	 */
	public getTokenAmount(marketIndex: number): BN {
		const spotPosition = this.getSpotPosition(marketIndex);
		if (spotPosition === undefined) {
			return ZERO;
		}
		const spotMarket = this.normalClient.getSpotMarketAccount(marketIndex);
		return getSignedTokenAmount(
			getTokenAmount(
				spotPosition.scaledBalance,
				spotMarket,
				spotPosition.balanceType
			),
			spotPosition.balanceType
		);
	}

	public getEmptyPosition(marketIndex: number): VaultPosition {
		return {
			baseAssetAmount: ZERO,
			remainderBaseAssetAmount: 0,
			lastCumulativeFundingRate: ZERO,
			marketIndex,
			quoteAssetAmount: ZERO,
			quoteEntryAmount: ZERO,
			quoteBreakEvenAmount: ZERO,
			openOrders: 0,
			openBids: ZERO,
			openAsks: ZERO,
			settledPnl: ZERO,
			lpShares: ZERO,
			lastBaseAssetAmountPerLp: ZERO,
			lastQuoteAssetAmountPerLp: ZERO,
			perLpBase: 0,
		};
	}

	public getClonedPosition(position: VaultPosition): VaultPosition {
		const clonedPosition = Object.assign({}, position);
		return clonedPosition;
	}

	public getUserAccountPublicKey(): PublicKey {
		return this.userAccountPublicKey;
	}

	public async exists(): Promise<boolean> {
		const userAccountRPCResponse =
			await this.normalClient.connection.getParsedAccountInfo(
				this.userAccountPublicKey
			);
		return userAccountRPCResponse.value !== null;
	}

	/**
	 * calculates the market position if the lp position was settled
	 * @returns : the settled userPosition
	 * @returns : the dust base asset amount (ie, < stepsize)
	 * @returns : pnl from settle
	 */
	public getVaultPositionWithLPSettle(
		marketIndex: number,
		originalPosition?: VaultPosition,
		burnLpShares = false,
		includeRemainderInBaseAmount = false
	): [VaultPosition, BN, BN] {
		originalPosition =
			originalPosition ??
			this.getVaultPosition(marketIndex) ??
			this.getEmptyPosition(marketIndex);

		if (originalPosition.lpShares.eq(ZERO)) {
			return [originalPosition, ZERO, ZERO];
		}

		const position = this.getClonedPosition(originalPosition);
		const market = this.normalClient.getPerpMarketAccount(position.marketIndex);

		if (amm.perLpBase != position.perLpBase) {
			// perLpBase = 1 => per 10 LP shares, perLpBase = -1 => per 0.1 LP shares
			const expoDiff = amm.perLpBase - position.perLpBase;
			const marketPerLpRebaseScalar = new BN(10 ** Math.abs(expoDiff));

			if (expoDiff > 0) {
				position.lastBaseAssetAmountPerLp =
					position.lastBaseAssetAmountPerLp.mul(marketPerLpRebaseScalar);
				position.lastQuoteAssetAmountPerLp =
					position.lastQuoteAssetAmountPerLp.mul(marketPerLpRebaseScalar);
			} else {
				position.lastBaseAssetAmountPerLp =
					position.lastBaseAssetAmountPerLp.div(marketPerLpRebaseScalar);
				position.lastQuoteAssetAmountPerLp =
					position.lastQuoteAssetAmountPerLp.div(marketPerLpRebaseScalar);
			}

			position.perLpBase = position.perLpBase + expoDiff;
		}

		const nShares = position.lpShares;

		// incorp unsettled funding on pre settled position
		const quoteFundingPnl = calculatePositionFundingPNL(market, position);

		let baseUnit = AMM_RESERVE_PRECISION;
		if (amm.perLpBase == position.perLpBase) {
			if (
				position.perLpBase >= 0 &&
				position.perLpBase <= AMM_RESERVE_PRECISION_EXP.toNumber()
			) {
				const marketPerLpRebase = new BN(10 ** amm.perLpBase);
				baseUnit = baseUnit.mul(marketPerLpRebase);
			} else if (
				position.perLpBase < 0 &&
				position.perLpBase >= -AMM_RESERVE_PRECISION_EXP.toNumber()
			) {
				const marketPerLpRebase = new BN(10 ** Math.abs(amm.perLpBase));
				baseUnit = baseUnit.div(marketPerLpRebase);
			} else {
				throw 'cannot calc';
			}
		} else {
			throw 'amm.perLpBase != position.perLpBase';
		}

		const deltaBaa = amm.baseAssetAmountPerLp
			.sub(position.lastBaseAssetAmountPerLp)
			.mul(nShares)
			.div(baseUnit);
		const deltaQaa = amm.quoteAssetAmountPerLp
			.sub(position.lastQuoteAssetAmountPerLp)
			.mul(nShares)
			.div(baseUnit);

		function sign(v: BN) {
			return v.isNeg() ? new BN(-1) : new BN(1);
		}

		function standardize(amount: BN, stepSize: BN) {
			const remainder = amount.abs().mod(stepSize).mul(sign(amount));
			const standardizedAmount = amount.sub(remainder);
			return [standardizedAmount, remainder];
		}

		const [standardizedBaa, remainderBaa] = standardize(
			deltaBaa,
			amm.orderStepSize
		);

		position.remainderBaseAssetAmount += remainderBaa.toNumber();

		if (
			Math.abs(position.remainderBaseAssetAmount) >
			amm.orderStepSize.toNumber()
		) {
			const [newStandardizedBaa, newRemainderBaa] = standardize(
				new BN(position.remainderBaseAssetAmount),
				amm.orderStepSize
			);
			position.baseAssetAmount =
				position.baseAssetAmount.add(newStandardizedBaa);
			position.remainderBaseAssetAmount = newRemainderBaa.toNumber();
		}

		let dustBaseAssetValue = ZERO;
		if (burnLpShares && position.remainderBaseAssetAmount != 0) {
			const oraclePriceData = this.normalClient.getOracleDataForMarket(
				position.marketIndex
			);
			dustBaseAssetValue = new BN(Math.abs(position.remainderBaseAssetAmount))
				.mul(oraclePriceData.price)
				.div(AMM_RESERVE_PRECISION)
				.add(ONE);
		}

		let updateType;
		if (position.baseAssetAmount.eq(ZERO)) {
			updateType = 'open';
		} else if (sign(position.baseAssetAmount).eq(sign(deltaBaa))) {
			updateType = 'increase';
		} else if (position.baseAssetAmount.abs().gt(deltaBaa.abs())) {
			updateType = 'reduce';
		} else if (position.baseAssetAmount.abs().eq(deltaBaa.abs())) {
			updateType = 'close';
		} else {
			updateType = 'flip';
		}

		let newQuoteEntry;
		let pnl;
		if (updateType == 'open' || updateType == 'increase') {
			newQuoteEntry = position.quoteEntryAmount.add(deltaQaa);
			pnl = ZERO;
		} else if (updateType == 'reduce' || updateType == 'close') {
			newQuoteEntry = position.quoteEntryAmount.sub(
				position.quoteEntryAmount
					.mul(deltaBaa.abs())
					.div(position.baseAssetAmount.abs())
			);
			pnl = position.quoteEntryAmount.sub(newQuoteEntry).add(deltaQaa);
		} else {
			newQuoteEntry = deltaQaa.sub(
				deltaQaa.mul(position.baseAssetAmount.abs()).div(deltaBaa.abs())
			);
			pnl = position.quoteEntryAmount.add(deltaQaa.sub(newQuoteEntry));
		}
		position.quoteEntryAmount = newQuoteEntry;
		position.baseAssetAmount = position.baseAssetAmount.add(standardizedBaa);
		position.quoteAssetAmount = position.quoteAssetAmount
			.add(deltaQaa)
			.add(quoteFundingPnl)
			.sub(dustBaseAssetValue);
		position.quoteBreakEvenAmount = position.quoteBreakEvenAmount
			.add(deltaQaa)
			.add(quoteFundingPnl)
			.sub(dustBaseAssetValue);

		// update open bids/asks
		const [marketOpenBids, marketOpenAsks] = calculateMarketOpenBidAsk(
			amm.baseAssetReserve,
			amm.minBaseAssetReserve,
			amm.maxBaseAssetReserve,
			amm.orderStepSize
		);
		const lpOpenBids = marketOpenBids
			.mul(position.lpShares)
			.div(amm.sqrtK);
		const lpOpenAsks = marketOpenAsks
			.mul(position.lpShares)
			.div(amm.sqrtK);
		position.openBids = lpOpenBids.add(position.openBids);
		position.openAsks = lpOpenAsks.add(position.openAsks);

		// eliminate counting funding on settled position
		if (position.baseAssetAmount.gt(ZERO)) {
			position.lastCumulativeFundingRate = amm.cumulativeFundingRateLong;
		} else if (position.baseAssetAmount.lt(ZERO)) {
			position.lastCumulativeFundingRate =
				amm.cumulativeFundingRateShort;
		} else {
			position.lastCumulativeFundingRate = ZERO;
		}

		const remainderBeforeRemoval = new BN(position.remainderBaseAssetAmount);

		if (includeRemainderInBaseAmount) {
			position.baseAssetAmount = position.baseAssetAmount.add(
				remainderBeforeRemoval
			);
			position.remainderBaseAssetAmount = 0;
		}

		return [position, remainderBeforeRemoval, pnl];
	}

	/**
	 * calculates Buying Power = free collateral / initial margin ratio
	 * @returns : Precision QUOTE_PRECISION
	 */
	public getPerpBuyingPower(marketIndex: number, collateralBuffer = ZERO): BN {
		const perpPosition = this.getVaultPositionWithLPSettle(
			marketIndex,
			undefined,
			true
		)[0];

		const perpMarket = this.normalClient.getPerpMarketAccount(marketIndex);
		const oraclePriceData = this.getOracleDataForMarket(marketIndex);
		const worstCaseBaseAssetAmount = perpPosition
			? calculateWorstCaseBaseAssetAmount(
					perpPosition,
					perpMarket,
					oraclePriceData.price
			  )
			: ZERO;

		const freeCollateral = this.getFreeCollateral().sub(collateralBuffer);

		return this.getPerpBuyingPowerFromFreeCollateralAndBaseAssetAmount(
			marketIndex,
			freeCollateral,
			worstCaseBaseAssetAmount
		);
	}

	getPerpBuyingPowerFromFreeCollateralAndBaseAssetAmount(
		marketIndex: number,
		freeCollateral: BN,
		baseAssetAmount: BN
	): BN {
		const marginRatio = calculateMarketMarginRatio(
			this.normalClient.getPerpMarketAccount(marketIndex),
			baseAssetAmount,
			'Initial',
			this.getUserAccount().maxMarginRatio
		);

		return freeCollateral.mul(MARGIN_PRECISION).div(new BN(marginRatio));
	}

	/**
	 * calculates Free Collateral = Total collateral - margin requirement
	 * @returns : Precision QUOTE_PRECISION
	 */
	public getFreeCollateral(marginCategory: MarginCategory = 'Initial'): BN {
		const totalCollateral = this.getTotalCollateral(marginCategory, true);
		const marginRequirement =
			marginCategory === 'Initial'
				? this.getInitialMarginRequirement()
				: this.getMaintenanceMarginRequirement();
		const freeCollateral = totalCollateral.sub(marginRequirement);
		return freeCollateral.gte(ZERO) ? freeCollateral : ZERO;
	}

	/**
	 * @returns The margin requirement of a certain type (Initial or Maintenance) in USDC. : QUOTE_PRECISION
	 */
	public getMarginRequirement(
		marginCategory: MarginCategory,
		liquidationBuffer?: BN,
		strict = false,
		includeOpenOrders = true
	): BN {
		return this.getTotalVaultPositionLiability(
			marginCategory,
			liquidationBuffer,
			includeOpenOrders,
			strict
		).add(
			this.getSpotMarketLiabilityValue(
				undefined,
				marginCategory,
				liquidationBuffer,
				includeOpenOrders,
				strict
			)
		);
	}

	/**
	 * @returns The initial margin requirement in USDC. : QUOTE_PRECISION
	 */
	public getInitialMarginRequirement(): BN {
		return this.getMarginRequirement('Initial', undefined, true);
	}

	/**
	 * @returns The maintenance margin requirement in USDC. : QUOTE_PRECISION
	 */
	public getMaintenanceMarginRequirement(): BN {
		// if user being liq'd, can continue to be liq'd until total collateral above the margin requirement plus buffer
		let liquidationBuffer = undefined;
		if (this.isBeingLiquidated()) {
			liquidationBuffer = new BN(
				this.normalClient.getStateAccount().liquidationMarginBufferRatio
			);
		}

		return this.getMarginRequirement('Maintenance', liquidationBuffer);
	}

	public getActiveVaultPositionsForUserAccount(
		userAccount: UserAccount
	): VaultPosition[] {
		return userAccount.vaultPositions.filter(
			(pos) =>
				!pos.baseAssetAmount.eq(ZERO) ||
				!pos.quoteAssetAmount.eq(ZERO) ||
				!(pos.openOrders == 0) ||
				!pos.lpShares.eq(ZERO)
		);
	}

	public getActiveVaultPositions(): VaultPosition[] {
		const userAccount = this.getUserAccount();
		return this.getActiveVaultPositionsForUserAccount(userAccount);
	}
	public getActiveVaultPositionsAndSlot(): DataAndSlot<VaultPosition[]> {
		const userAccount = this.getUserAccountAndSlot();
		const positions = this.getActiveVaultPositionsForUserAccount(
			userAccount.data
		);
		return {
			data: positions,
			slot: userAccount.slot,
		};
	}

	public getActiveSpotPositionsForUserAccount(
		userAccount: UserAccount
	): SpotPosition[] {
		return userAccount.spotPositions.filter(
			(pos) => !isSpotPositionAvailable(pos)
		);
	}

	public getActiveSpotPositions(): SpotPosition[] {
		const userAccount = this.getUserAccount();
		return this.getActiveSpotPositionsForUserAccount(userAccount);
	}
	public getActiveSpotPositionsAndSlot(): DataAndSlot<SpotPosition[]> {
		const userAccount = this.getUserAccountAndSlot();
		const positions = this.getActiveSpotPositionsForUserAccount(
			userAccount.data
		);
		return {
			data: positions,
			slot: userAccount.slot,
		};
	}

	public getSpotMarketAssetAndLiabilityValue(
		marketIndex?: number,
		marginCategory?: MarginCategory,
		liquidationBuffer?: BN,
		includeOpenOrders?: boolean,
		strict = false,
		now?: BN
	): { totalAssetValue: BN; totalLiabilityValue: BN } {
		now = now || new BN(new Date().getTime() / 1000);
		let netQuoteValue = ZERO;
		let totalAssetValue = ZERO;
		let totalLiabilityValue = ZERO;
		for (const spotPosition of this.getUserAccount().spotPositions) {
			const countForBase =
				marketIndex === undefined || spotPosition.marketIndex === marketIndex;

			const countForQuote =
				marketIndex === undefined ||
				marketIndex === QUOTE_SPOT_MARKET_INDEX ||
				(includeOpenOrders && spotPosition.openOrders !== 0);
			if (
				isSpotPositionAvailable(spotPosition) ||
				(!countForBase && !countForQuote)
			) {
				continue;
			}

			const spotMarketAccount: SpotMarketAccount =
				this.normalClient.getSpotMarketAccount(spotPosition.marketIndex);

			const oraclePriceData = this.getOracleDataForSpotMarket(
				spotPosition.marketIndex
			);

			let twap5min;
			if (strict) {
				twap5min = calculateLiveOracleTwap(
					spotMarketAccount.historicalOracleData,
					oraclePriceData,
					now,
					FIVE_MINUTE // 5MIN
				);
			}
			const strictOraclePrice = new StrictOraclePrice(
				oraclePriceData.price,
				twap5min
			);

			if (
				spotPosition.marketIndex === QUOTE_SPOT_MARKET_INDEX &&
				countForQuote
			) {
				const tokenAmount = getSignedTokenAmount(
					getTokenAmount(
						spotPosition.scaledBalance,
						spotMarketAccount,
						spotPosition.balanceType
					),
					spotPosition.balanceType
				);

				if (isVariant(spotPosition.balanceType, 'borrow')) {
					const weightedTokenValue = this.getSpotLiabilityValue(
						tokenAmount,
						strictOraclePrice,
						spotMarketAccount,
						marginCategory,
						liquidationBuffer
					).abs();

					netQuoteValue = netQuoteValue.sub(weightedTokenValue);
				} else {
					const weightedTokenValue = this.getSpotAssetValue(
						tokenAmount,
						strictOraclePrice,
						spotMarketAccount,
						marginCategory
					);

					netQuoteValue = netQuoteValue.add(weightedTokenValue);
				}

				continue;
			}

			if (!includeOpenOrders && countForBase) {
				if (isVariant(spotPosition.balanceType, 'borrow')) {
					const tokenAmount = getSignedTokenAmount(
						getTokenAmount(
							spotPosition.scaledBalance,
							spotMarketAccount,
							spotPosition.balanceType
						),
						SpotBalanceType.BORROW
					);
					const liabilityValue = this.getSpotLiabilityValue(
						tokenAmount,
						strictOraclePrice,
						spotMarketAccount,
						marginCategory,
						liquidationBuffer
					).abs();
					totalLiabilityValue = totalLiabilityValue.add(liabilityValue);

					continue;
				} else {
					const tokenAmount = getTokenAmount(
						spotPosition.scaledBalance,
						spotMarketAccount,
						spotPosition.balanceType
					);
					const assetValue = this.getSpotAssetValue(
						tokenAmount,
						strictOraclePrice,
						spotMarketAccount,
						marginCategory
					);
					totalAssetValue = totalAssetValue.add(assetValue);

					continue;
				}
			}

			const {
				tokenAmount: worstCaseTokenAmount,
				ordersValue: worstCaseQuoteTokenAmount,
			} = getWorstCaseTokenAmounts(
				spotPosition,
				spotMarketAccount,
				strictOraclePrice,
				marginCategory,
				this.getUserAccount().maxMarginRatio
			);

			if (worstCaseTokenAmount.gt(ZERO) && countForBase) {
				const baseAssetValue = this.getSpotAssetValue(
					worstCaseTokenAmount,
					strictOraclePrice,
					spotMarketAccount,
					marginCategory
				);

				totalAssetValue = totalAssetValue.add(baseAssetValue);
			}

			if (worstCaseTokenAmount.lt(ZERO) && countForBase) {
				const baseLiabilityValue = this.getSpotLiabilityValue(
					worstCaseTokenAmount,
					strictOraclePrice,
					spotMarketAccount,
					marginCategory,
					liquidationBuffer
				).abs();

				totalLiabilityValue = totalLiabilityValue.add(baseLiabilityValue);
			}

			if (worstCaseQuoteTokenAmount.gt(ZERO) && countForQuote) {
				netQuoteValue = netQuoteValue.add(worstCaseQuoteTokenAmount);
			}

			if (worstCaseQuoteTokenAmount.lt(ZERO) && countForQuote) {
				let weight = SPOT_MARKET_WEIGHT_PRECISION;
				if (marginCategory === 'Initial') {
					weight = BN.max(weight, new BN(this.getUserAccount().maxMarginRatio));
				}

				const weightedTokenValue = worstCaseQuoteTokenAmount
					.abs()
					.mul(weight)
					.div(SPOT_MARKET_WEIGHT_PRECISION);

				netQuoteValue = netQuoteValue.sub(weightedTokenValue);
			}

			totalLiabilityValue = totalLiabilityValue.add(
				new BN(spotPosition.openOrders).mul(OPEN_ORDER_MARGIN_REQUIREMENT)
			);
		}

		if (marketIndex === undefined || marketIndex === QUOTE_SPOT_MARKET_INDEX) {
			if (netQuoteValue.gt(ZERO)) {
				totalAssetValue = totalAssetValue.add(netQuoteValue);
			} else {
				totalLiabilityValue = totalLiabilityValue.add(netQuoteValue.abs());
			}
		}

		return { totalAssetValue, totalLiabilityValue };
	}

	public getSpotMarketLiabilityValue(
		marketIndex?: number,
		marginCategory?: MarginCategory,
		liquidationBuffer?: BN,
		includeOpenOrders?: boolean,
		strict = false,
		now?: BN
	): BN {
		const { totalLiabilityValue } = this.getSpotMarketAssetAndLiabilityValue(
			marketIndex,
			marginCategory,
			liquidationBuffer,
			includeOpenOrders,
			strict,
			now
		);
		return totalLiabilityValue;
	}

	getSpotLiabilityValue(
		tokenAmount: BN,
		strictOraclePrice: StrictOraclePrice,
		spotMarketAccount: SpotMarketAccount,
		marginCategory?: MarginCategory,
		liquidationBuffer?: BN
	): BN {
		let liabilityValue = getStrictTokenValue(
			tokenAmount,
			spotMarketAccount.decimals,
			strictOraclePrice
		);

		if (marginCategory !== undefined) {
			let weight = calculateLiabilityWeight(
				tokenAmount,
				spotMarketAccount,
				marginCategory
			);

			if (
				marginCategory === 'Initial' &&
				spotMarketAccount.marketIndex !== QUOTE_SPOT_MARKET_INDEX
			) {
				weight = BN.max(
					weight,
					SPOT_MARKET_WEIGHT_PRECISION.addn(
						this.getUserAccount().maxMarginRatio
					)
				);
			}

			if (liquidationBuffer !== undefined) {
				weight = weight.add(liquidationBuffer);
			}

			liabilityValue = liabilityValue
				.mul(weight)
				.div(SPOT_MARKET_WEIGHT_PRECISION);
		}

		return liabilityValue;
	}

	public getSpotMarketAssetValue(
		marketIndex?: number,
		marginCategory?: MarginCategory,
		includeOpenOrders?: boolean,
		strict = false,
		now?: BN
	): BN {
		const { totalAssetValue } = this.getSpotMarketAssetAndLiabilityValue(
			marketIndex,
			marginCategory,
			undefined,
			includeOpenOrders,
			strict,
			now
		);
		return totalAssetValue;
	}

	getSpotAssetValue(
		tokenAmount: BN,
		strictOraclePrice: StrictOraclePrice,
		spotMarketAccount: SpotMarketAccount,
		marginCategory?: MarginCategory
	): BN {
		let assetValue = getStrictTokenValue(
			tokenAmount,
			spotMarketAccount.decimals,
			strictOraclePrice
		);

		if (marginCategory !== undefined) {
			let weight = calculateAssetWeight(
				tokenAmount,
				strictOraclePrice.current,
				spotMarketAccount,
				marginCategory
			);

			if (
				marginCategory === 'Initial' &&
				spotMarketAccount.marketIndex !== QUOTE_SPOT_MARKET_INDEX
			) {
				const userCustomAssetWeight = BN.max(
					ZERO,
					SPOT_MARKET_WEIGHT_PRECISION.subn(
						this.getUserAccount().maxMarginRatio
					)
				);
				weight = BN.min(weight, userCustomAssetWeight);
			}

			assetValue = assetValue.mul(weight).div(SPOT_MARKET_WEIGHT_PRECISION);
		}

		return assetValue;
	}

	public getSpotPositionValue(
		marketIndex: number,
		marginCategory?: MarginCategory,
		includeOpenOrders?: boolean,
		strict = false,
		now?: BN
	): BN {
		const { totalAssetValue, totalLiabilityValue } =
			this.getSpotMarketAssetAndLiabilityValue(
				marketIndex,
				marginCategory,
				undefined,
				includeOpenOrders,
				strict,
				now
			);

		return totalAssetValue.sub(totalLiabilityValue);
	}

	public getNetSpotMarketValue(withWeightMarginCategory?: MarginCategory): BN {
		const { totalAssetValue, totalLiabilityValue } =
			this.getSpotMarketAssetAndLiabilityValue(
				undefined,
				withWeightMarginCategory
			);

		return totalAssetValue.sub(totalLiabilityValue);
	}

	/**
	 * calculates TotalCollateral: collateral + unrealized pnl
	 * @returns : Precision QUOTE_PRECISION
	 */
	public getTotalCollateral(
		marginCategory: MarginCategory = 'Initial',
		strict = false
	): BN {
		return this.getSpotMarketAssetValue(
			undefined,
			marginCategory,
			true,
			strict
		).add(this.getUnrealizedPNL(true, undefined, marginCategory, strict));
	}

	/**
	 * calculates User Health by comparing total collateral and maint. margin requirement
	 * @returns : number (value from [0, 100])
	 */
	public getHealth(): number {
		if (this.isBeingLiquidated()) {
			return 0;
		}

		const totalCollateral = this.getTotalCollateral('Maintenance');
		const maintenanceMarginReq = this.getMaintenanceMarginRequirement();

		let health: number;

		if (maintenanceMarginReq.eq(ZERO) && totalCollateral.gte(ZERO)) {
			health = 100;
		} else if (totalCollateral.lte(ZERO)) {
			health = 0;
		} else {
			health = Math.round(
				Math.min(
					100,
					Math.max(
						0,
						(1 - maintenanceMarginReq.toNumber() / totalCollateral.toNumber()) *
							100
					)
				)
			);
		}

		return health;
	}

	calculateWeightedVaultPositionLiability(
		perpPosition: VaultPosition,
		marginCategory?: MarginCategory,
		liquidationBuffer?: BN,
		includeOpenOrders?: boolean,
		strict = false
	): BN {
		const market = this.normalClient.getPerpMarketAccount(
			perpPosition.marketIndex
		);

		if (perpPosition.lpShares.gt(ZERO)) {
			// is an lp, clone so we dont mutate the position
			perpPosition = this.getVaultPositionWithLPSettle(
				market.marketIndex,
				this.getClonedPosition(perpPosition),
				!!marginCategory
			)[0];
		}

		let valuationPrice = this.getOracleDataForMarket(
			market.marketIndex
		).price;

		if (isVariant(market.status, 'settlement')) {
			valuationPrice = market.expiryPrice;
		}

		let baseAssetAmount: BN;
		let liabilityValue;
		if (includeOpenOrders) {
			const { worstCaseBaseAssetAmount, worstCaseLiabilityValue } =
				calculateWorstCasePerpLiabilityValue(
					perpPosition,
					market,
					valuationPrice
				);
			baseAssetAmount = worstCaseBaseAssetAmount;
			liabilityValue = worstCaseLiabilityValue;
		} else {
			baseAssetAmount = perpPosition.baseAssetAmount;
			liabilityValue = calculatePerpLiabilityValue(
				baseAssetAmount,
				valuationPrice,
				isVariant(market.contractType, 'prediction')
			);
		}

		if (marginCategory) {
			let marginRatio = new BN(
				calculateMarketMarginRatio(
					market,
					baseAssetAmount.abs(),
					marginCategory,
					this.getUserAccount().maxMarginRatio
				)
			);

			if (liquidationBuffer !== undefined) {
				marginRatio = marginRatio.add(liquidationBuffer);
			}

			if (isVariant(market.status, 'settlement')) {
				marginRatio = ZERO;
			}

			const quoteSpotMarket = this.normalClient.getSpotMarketAccount(
				market.quoteSpotMarketIndex
			);
			const quoteOraclePriceData = this.normalClient.getOracleDataForSpotMarket(
				QUOTE_SPOT_MARKET_INDEX
			);

			let quotePrice;
			if (strict) {
				quotePrice = BN.max(
					quoteOraclePriceData.price,
					quoteSpotMarket.historicalOracleData.lastOraclePriceTwap5Min
				);
			} else {
				quotePrice = quoteOraclePriceData.price;
			}

			liabilityValue = liabilityValue
				.mul(quotePrice)
				.div(PRICE_PRECISION)
				.mul(marginRatio)
				.div(MARGIN_PRECISION);

			if (includeOpenOrders) {
				liabilityValue = liabilityValue.add(
					new BN(perpPosition.openOrders).mul(OPEN_ORDER_MARGIN_REQUIREMENT)
				);

				if (perpPosition.lpShares.gt(ZERO)) {
					liabilityValue = liabilityValue.add(
						BN.max(
							QUOTE_PRECISION,
							valuationPrice
								.mul(amm.orderStepSize)
								.mul(QUOTE_PRECISION)
								.div(AMM_RESERVE_PRECISION)
								.div(PRICE_PRECISION)
						)
					);
				}
			}
		}

		return liabilityValue;
	}

	/**
	 * calculates position value of a single perp market in margin system
	 * @returns : Precision QUOTE_PRECISION
	 */
	public getPerpMarketLiabilityValue(
		marketIndex: number,
		marginCategory?: MarginCategory,
		liquidationBuffer?: BN,
		includeOpenOrders?: boolean,
		strict = false
	): BN {
		const perpPosition = this.getVaultPosition(marketIndex);
		return this.calculateWeightedVaultPositionLiability(
			perpPosition,
			marginCategory,
			liquidationBuffer,
			includeOpenOrders,
			strict
		);
	}

	/**
	 * calculates sum of position value across all positions in margin system
	 * @returns : Precision QUOTE_PRECISION
	 */
	getTotalVaultPositionLiability(
		marginCategory?: MarginCategory,
		liquidationBuffer?: BN,
		includeOpenOrders?: boolean,
		strict = false
	): BN {
		return this.getActiveVaultPositions().reduce(
			(totalPerpValue, perpPosition) => {
				const baseAssetValue = this.calculateWeightedVaultPositionLiability(
					perpPosition,
					marginCategory,
					liquidationBuffer,
					includeOpenOrders,
					strict
				);
				return totalPerpValue.add(baseAssetValue);
			},
			ZERO
		);
	}

	/**
	 * calculates position value based on oracle
	 * @returns : Precision QUOTE_PRECISION
	 */
	public getVaultPositionValue(
		marketIndex: number,
		oraclePriceData: OraclePriceData,
		includeOpenOrders = false
	): BN {
		const userPosition =
			this.getVaultPositionWithLPSettle(
				marketIndex,
				undefined,
				false,
				true
			)[0] || this.getEmptyPosition(marketIndex);
		const market = this.normalClient.getPerpMarketAccount(
			userPosition.marketIndex
		);
		return calculateBaseAssetValueWithOracle(
			market,
			userPosition,
			oraclePriceData,
			includeOpenOrders
		);
	}

	/**
	 * calculates position liabiltiy value in margin system
	 * @returns : Precision QUOTE_PRECISION
	 */
	public getPerpLiabilityValue(
		marketIndex: number,
		oraclePriceData: OraclePriceData,
		includeOpenOrders = false
	): BN {
		const userPosition =
			this.getVaultPositionWithLPSettle(
				marketIndex,
				undefined,
				false,
				true
			)[0] || this.getEmptyPosition(marketIndex);
		const market = this.normalClient.getPerpMarketAccount(
			userPosition.marketIndex
		);

		if (includeOpenOrders) {
			return calculateWorstCasePerpLiabilityValue(
				userPosition,
				market,
				oraclePriceData.price
			).worstCaseLiabilityValue;
		} else {
			return calculatePerpLiabilityValue(
				userPosition.baseAssetAmount,
				oraclePriceData.price,
				isVariant(market.contractType, 'prediction')
			);
		}
	}

	public getPositionSide(
		currentPosition: Pick<VaultPosition, 'baseAssetAmount'>
	): PositionDirection | undefined {
		if (currentPosition.baseAssetAmount.gt(ZERO)) {
			return PositionDirection.LONG;
		} else if (currentPosition.baseAssetAmount.lt(ZERO)) {
			return PositionDirection.SHORT;
		} else {
			return undefined;
		}
	}

	/**
	 * calculates average exit price (optionally for closing up to 100% of position)
	 * @returns : Precision PRICE_PRECISION
	 */
	public getPositionEstimatedExitPriceAndPnl(
		position: VaultPosition,
		amountToClose?: BN,
		useAMMClose = false
	): [BN, BN] {
		const market = this.normalClient.getPerpMarketAccount(position.marketIndex);

		const entryPrice = calculateEntryPrice(position);

		const oraclePriceData = this.getOracleDataForMarket(
			position.marketIndex
		);

		if (amountToClose) {
			if (amountToClose.eq(ZERO)) {
				return [calculateReservePrice(market, oraclePriceData), ZERO];
			}
			position = {
				baseAssetAmount: amountToClose,
				lastCumulativeFundingRate: position.lastCumulativeFundingRate,
				marketIndex: position.marketIndex,
				quoteAssetAmount: position.quoteAssetAmount,
			} as VaultPosition;
		}

		let baseAssetValue: BN;

		if (useAMMClose) {
			baseAssetValue = calculateBaseAssetValue(
				market,
				position,
				oraclePriceData
			);
		} else {
			baseAssetValue = calculateBaseAssetValueWithOracle(
				market,
				position,
				oraclePriceData
			);
		}
		if (position.baseAssetAmount.eq(ZERO)) {
			return [ZERO, ZERO];
		}

		const exitPrice = baseAssetValue
			.mul(AMM_TO_QUOTE_PRECISION_RATIO)
			.mul(PRICE_PRECISION)
			.div(position.baseAssetAmount.abs());

		const pnlPerBase = exitPrice.sub(entryPrice);
		const pnl = pnlPerBase
			.mul(position.baseAssetAmount)
			.div(PRICE_PRECISION)
			.div(AMM_TO_QUOTE_PRECISION_RATIO);

		return [exitPrice, pnl];
	}

	/**
	 * calculates current user leverage which is (total liability size) / (net asset value)
	 * @returns : Precision TEN_THOUSAND
	 */
	public getLeverage(includeOpenOrders = true): BN {
		return this.calculateLeverageFromComponents(
			this.getLeverageComponents(includeOpenOrders)
		);
	}

	calculateLeverageFromComponents({
		perpLiabilityValue,
		perpPnl,
		spotAssetValue,
		spotLiabilityValue,
	}: {
		perpLiabilityValue: BN;
		perpPnl: BN;
		spotAssetValue: BN;
		spotLiabilityValue: BN;
	}): BN {
		const totalLiabilityValue = perpLiabilityValue.add(spotLiabilityValue);
		const totalAssetValue = spotAssetValue.add(perpPnl);
		const netAssetValue = totalAssetValue.sub(spotLiabilityValue);

		if (netAssetValue.eq(ZERO)) {
			return ZERO;
		}

		return totalLiabilityValue.mul(TEN_THOUSAND).div(netAssetValue);
	}

	getLeverageComponents(
		includeOpenOrders = true,
		marginCategory: MarginCategory = undefined
	): {
		perpLiabilityValue: BN;
		perpPnl: BN;
		spotAssetValue: BN;
		spotLiabilityValue: BN;
	} {
		const perpLiability = this.getTotalVaultPositionLiability(
			marginCategory,
			undefined,
			includeOpenOrders
		);
		const perpPnl = this.getUnrealizedPNL(true, undefined, marginCategory);

		const {
			totalAssetValue: spotAssetValue,
			totalLiabilityValue: spotLiabilityValue,
		} = this.getSpotMarketAssetAndLiabilityValue(
			undefined,
			marginCategory,
			undefined,
			includeOpenOrders
		);

		return {
			perpLiabilityValue: perpLiability,
			perpPnl,
			spotAssetValue,
			spotLiabilityValue,
		};
	}

	isDustDepositPosition(spotMarketAccount: SpotMarketAccount): boolean {
		const marketIndex = spotMarketAccount.marketIndex;

		const spotPosition = this.getSpotPosition(spotMarketAccount.marketIndex);

		if (isSpotPositionAvailable(spotPosition)) {
			return false;
		}

		const depositAmount = this.getTokenAmount(spotMarketAccount.marketIndex);

		if (depositAmount.lte(ZERO)) {
			return false;
		}

		const oraclePriceData = this.getOracleDataForSpotMarket(marketIndex);

		const strictOraclePrice = new StrictOraclePrice(
			oraclePriceData.price,
			oraclePriceData.twap
		);

		const balanceValue = this.getSpotAssetValue(
			depositAmount,
			strictOraclePrice,
			spotMarketAccount
		);

		if (balanceValue.lt(DUST_POSITION_SIZE)) {
			return true;
		}

		return false;
	}

	getSpotMarketAccountsWithDustPosition() {
		const spotMarketAccounts = this.normalClient.getSpotMarketAccounts();

		const dustPositionAccounts: SpotMarketAccount[] = [];

		for (const spotMarketAccount of spotMarketAccounts) {
			const isDust = this.isDustDepositPosition(spotMarketAccount);
			if (isDust) {
				dustPositionAccounts.push(spotMarketAccount);
			}
		}

		return dustPositionAccounts;
	}

	getTotalLiabilityValue(marginCategory?: MarginCategory): BN {
		return this.getTotalVaultPositionLiability(
			marginCategory,
			undefined,
			true
		).add(
			this.getSpotMarketLiabilityValue(
				undefined,
				marginCategory,
				undefined,
				true
			)
		);
	}

	getTotalAssetValue(marginCategory?: MarginCategory): BN {
		return this.getSpotMarketAssetValue(undefined, marginCategory, true).add(
			this.getUnrealizedPNL(true, undefined, marginCategory)
		);
	}

	getNetUsdValue(): BN {
		const netSpotValue = this.getNetSpotMarketValue();
		const unrealizedPnl = this.getUnrealizedPNL(true, undefined, undefined);
		return netSpotValue.add(unrealizedPnl);
	}

	/**
	 * Calculates the all time P&L of the user.
	 *
	 * Net withdraws + Net spot market value + Net unrealized P&L -
	 */
	getTotalAllTimePnl(): BN {
		const netUsdValue = this.getNetUsdValue();
		const totalDeposits = this.getUserAccount().totalDeposits;
		const totalWithdraws = this.getUserAccount().totalWithdraws;

		const totalPnl = netUsdValue.add(totalWithdraws).sub(totalDeposits);

		return totalPnl;
	}

	/**
	 * calculates max allowable leverage exceeding hitting requirement category
	 * for large sizes where imf factor activates, result is a lower bound
	 * @param marginCategory {Initial, Maintenance}
	 * @param isLp if calculating max leveraging for adding lp, need to add buffer
	 * @returns : Precision TEN_THOUSAND
	 */
	public getMaxLeverageForPerp(
		perpMarketIndex: number,
		marginCategory: MarginCategory = 'Initial',
		isLp = false
	): BN {
		const market = this.normalClient.getPerpMarketAccount(perpMarketIndex);
		const marketPrice =
			this.normalClient.getOracleDataForMarket(perpMarketIndex).price;

		const { perpLiabilityValue, perpPnl, spotAssetValue, spotLiabilityValue } =
			this.getLeverageComponents();

		const totalAssetValue = spotAssetValue.add(perpPnl);

		const netAssetValue = totalAssetValue.sub(spotLiabilityValue);

		if (netAssetValue.eq(ZERO)) {
			return ZERO;
		}

		const totalLiabilityValue = perpLiabilityValue.add(spotLiabilityValue);

		const lpBuffer = isLp
			? marketPrice.mul(amm.orderStepSize).div(AMM_RESERVE_PRECISION)
			: ZERO;

		const freeCollateral = this.getFreeCollateral().sub(lpBuffer);

		let rawMarginRatio;

		switch (marginCategory) {
			case 'Initial':
				rawMarginRatio = Math.max(
					market.marginRatioInitial,
					this.getUserAccount().maxMarginRatio
				);
				break;
			case 'Maintenance':
				rawMarginRatio = market.marginRatioMaintenance;
				break;
			default:
				rawMarginRatio = market.marginRatioInitial;
				break;
		}

		// absolute max fesible size (upper bound)
		const maxSize = BN.max(
			ZERO,
			freeCollateral
				.mul(MARGIN_PRECISION)
				.div(new BN(rawMarginRatio))
				.mul(PRICE_PRECISION)
				.div(marketPrice)
		);

		// margin ratio incorporting upper bound on size
		let marginRatio = calculateMarketMarginRatio(
			market,
			maxSize,
			marginCategory,
			this.getUserAccount().maxMarginRatio
		);

		// use more fesible size since imf factor activated
		let attempts = 0;
		while (marginRatio > rawMarginRatio + 1e-4 && attempts < 10) {
			// more fesible size (upper bound)
			const targetSize = BN.max(
				ZERO,
				freeCollateral
					.mul(MARGIN_PRECISION)
					.div(new BN(marginRatio))
					.mul(PRICE_PRECISION)
					.div(marketPrice)
			);

			// margin ratio incorporting more fesible target size
			marginRatio = calculateMarketMarginRatio(
				market,
				targetSize,
				marginCategory,
				this.getUserAccount().maxMarginRatio
			);
			attempts += 1;
		}

		// how much more liabilities can be opened w remaining free collateral
		const additionalLiabilities = freeCollateral
			.mul(MARGIN_PRECISION)
			.div(new BN(marginRatio));

		return totalLiabilityValue
			.add(additionalLiabilities)
			.mul(TEN_THOUSAND)
			.div(netAssetValue);
	}

	/**
	 * calculates max allowable leverage exceeding hitting requirement category
	 * @param spotMarketIndex
	 * @param direction
	 * @returns : Precision TEN_THOUSAND
	 */
	public getMaxLeverageForSpot(
		spotMarketIndex: number,
		direction: PositionDirection
	): BN {
		const { perpLiabilityValue, perpPnl, spotAssetValue, spotLiabilityValue } =
			this.getLeverageComponents();

		const totalLiabilityValue = perpLiabilityValue.add(spotLiabilityValue);
		const totalAssetValue = spotAssetValue.add(perpPnl);

		const netAssetValue = totalAssetValue.sub(spotLiabilityValue);

		if (netAssetValue.eq(ZERO)) {
			return ZERO;
		}

		const currentQuoteAssetValue = this.getSpotMarketAssetValue(
			QUOTE_SPOT_MARKET_INDEX
		);
		const currentQuoteLiabilityValue = this.getSpotMarketLiabilityValue(
			QUOTE_SPOT_MARKET_INDEX
		);
		const currentQuoteValue = currentQuoteAssetValue.sub(
			currentQuoteLiabilityValue
		);

		const currentSpotMarketAssetValue =
			this.getSpotMarketAssetValue(spotMarketIndex);
		const currentSpotMarketLiabilityValue =
			this.getSpotMarketLiabilityValue(spotMarketIndex);
		const currentSpotMarketNetValue = currentSpotMarketAssetValue.sub(
			currentSpotMarketLiabilityValue
		);

		const tradeQuoteAmount = this.getMaxTradeSizeUSDCForSpot(
			spotMarketIndex,
			direction,
			currentQuoteAssetValue,
			currentSpotMarketNetValue
		);

		let assetValueToAdd = ZERO;
		let liabilityValueToAdd = ZERO;

		const newQuoteNetValue = isVariant(direction, 'short')
			? currentQuoteValue.add(tradeQuoteAmount)
			: currentQuoteValue.sub(tradeQuoteAmount);
		const newQuoteAssetValue = BN.max(newQuoteNetValue, ZERO);
		const newQuoteLiabilityValue = BN.min(newQuoteNetValue, ZERO).abs();

		assetValueToAdd = assetValueToAdd.add(
			newQuoteAssetValue.sub(currentQuoteAssetValue)
		);
		liabilityValueToAdd = liabilityValueToAdd.add(
			newQuoteLiabilityValue.sub(currentQuoteLiabilityValue)
		);

		const newSpotMarketNetValue = isVariant(direction, 'long')
			? currentSpotMarketNetValue.add(tradeQuoteAmount)
			: currentSpotMarketNetValue.sub(tradeQuoteAmount);
		const newSpotMarketAssetValue = BN.max(newSpotMarketNetValue, ZERO);
		const newSpotMarketLiabilityValue = BN.min(
			newSpotMarketNetValue,
			ZERO
		).abs();

		assetValueToAdd = assetValueToAdd.add(
			newSpotMarketAssetValue.sub(currentSpotMarketAssetValue)
		);
		liabilityValueToAdd = liabilityValueToAdd.add(
			newSpotMarketLiabilityValue.sub(currentSpotMarketLiabilityValue)
		);

		const finalTotalAssetValue = totalAssetValue.add(assetValueToAdd);
		const finalTotalSpotLiability = spotLiabilityValue.add(liabilityValueToAdd);

		const finalTotalLiabilityValue =
			totalLiabilityValue.add(liabilityValueToAdd);

		const finalNetAssetValue = finalTotalAssetValue.sub(
			finalTotalSpotLiability
		);

		return finalTotalLiabilityValue.mul(TEN_THOUSAND).div(finalNetAssetValue);
	}

	/**
	 * calculates margin ratio: 1 / leverage
	 * @returns : Precision TEN_THOUSAND
	 */
	public getMarginRatio(): BN {
		const { perpLiabilityValue, perpPnl, spotAssetValue, spotLiabilityValue } =
			this.getLeverageComponents();

		const totalLiabilityValue = perpLiabilityValue.add(spotLiabilityValue);
		const totalAssetValue = spotAssetValue.add(perpPnl);

		if (totalLiabilityValue.eq(ZERO)) {
			return BN_MAX;
		}

		const netAssetValue = totalAssetValue.sub(spotLiabilityValue);

		return netAssetValue.mul(TEN_THOUSAND).div(totalLiabilityValue);
	}

	public canBeLiquidated(): {
		canBeLiquidated: boolean;
		marginRequirement: BN;
		totalCollateral: BN;
	} {
		const totalCollateral = this.getTotalCollateral('Maintenance');

		const marginRequirement = this.getMaintenanceMarginRequirement();
		const canBeLiquidated = totalCollateral.lt(marginRequirement);

		return {
			canBeLiquidated,
			marginRequirement,
			totalCollateral,
		};
	}

	public isBeingLiquidated(): boolean {
		return (
			(this.getUserAccount().status &
				(UserStatus.BEING_LIQUIDATED | UserStatus.BANKRUPT)) >
			0
		);
	}

	public hasStatus(status: UserStatus): boolean {
		return (this.getUserAccount().status & status) > 0;
	}

	public isBankrupt(): boolean {
		return (this.getUserAccount().status & UserStatus.BANKRUPT) > 0;
	}

	/**
	 * Calculate the liquidation price of a spot position
	 * @param marketIndex
	 * @returns Precision : PRICE_PRECISION
	 */
	public spotLiquidationPrice(
		marketIndex: number,
		positionBaseSizeChange: BN = ZERO
	): BN {
		const currentSpotPosition = this.getSpotPosition(marketIndex);

		if (!currentSpotPosition) {
			return new BN(-1);
		}

		const totalCollateral = this.getTotalCollateral('Maintenance');
		const maintenanceMarginRequirement = this.getMaintenanceMarginRequirement();
		const freeCollateral = BN.max(
			ZERO,
			totalCollateral.sub(maintenanceMarginRequirement)
		);

		const market = this.normalClient.getSpotMarketAccount(marketIndex);
		let signedTokenAmount = getSignedTokenAmount(
			getTokenAmount(
				currentSpotPosition.scaledBalance,
				market,
				currentSpotPosition.balanceType
			),
			currentSpotPosition.balanceType
		);
		signedTokenAmount = signedTokenAmount.add(positionBaseSizeChange);

		if (signedTokenAmount.eq(ZERO)) {
			return new BN(-1);
		}

		let freeCollateralDelta = this.calculateFreeCollateralDeltaForSpot(
			market,
			signedTokenAmount
		);

		const oracle = market.oracle;
		const perpMarketWithSameOracle = this.normalClient
			.getPerpMarketAccounts()
			.find((market) => amm.oracle.equals(oracle));
		const oraclePrice =
			this.normalClient.getOracleDataForSpotMarket(marketIndex).price;
		if (perpMarketWithSameOracle) {
			const perpPosition = this.getVaultPositionWithLPSettle(
				perpMarketWithSameOracle.marketIndex,
				undefined,
				true
			)[0];
			if (perpPosition) {
				const freeCollateralDeltaForPerp =
					this.calculateFreeCollateralDeltaForPerp(
						perpMarketWithSameOracle,
						perpPosition,
						ZERO,
						oraclePrice
					);

				freeCollateralDelta = freeCollateralDelta.add(
					freeCollateralDeltaForPerp || ZERO
				);
			}
		}

		if (freeCollateralDelta.eq(ZERO)) {
			return new BN(-1);
		}

		const liqPriceDelta = freeCollateral
			.mul(QUOTE_PRECISION)
			.div(freeCollateralDelta);

		const liqPrice = oraclePrice.sub(liqPriceDelta);

		if (liqPrice.lt(ZERO)) {
			return new BN(-1);
		}

		return liqPrice;
	}

	/**
	 * Calculate the liquidation price of a perp position, with optional parameter to calculate the liquidation price after a trade
	 * @param marketIndex
	 * @param positionBaseSizeChange // change in position size to calculate liquidation price for : Precision 10^9
	 * @param estimatedEntryPrice
	 * @param marginCategory // allow Initial to be passed in if we are trying to calculate price for DLP de-risking
	 * @param includeOpenOrders
	 * @param offsetCollateral // allows calculating the liquidation price after this offset collateral is added to the user's account (e.g. : what will the liquidation price be for this position AFTER I deposit $x worth of collateral)
	 * @returns Precision : PRICE_PRECISION
	 */
	public liquidationPrice(
		marketIndex: number,
		positionBaseSizeChange: BN = ZERO,
		estimatedEntryPrice: BN = ZERO,
		marginCategory: MarginCategory = 'Maintenance',
		includeOpenOrders = false,
		offsetCollateral = ZERO
	): BN {
		const totalCollateral = this.getTotalCollateral(marginCategory);
		const marginRequirement = this.getMarginRequirement(
			marginCategory,
			undefined,
			false,
			includeOpenOrders
		);
		let freeCollateral = BN.max(
			ZERO,
			totalCollateral.sub(marginRequirement)
		).add(offsetCollateral);

		const oracle =
			this.normalClient.getPerpMarketAccount(marketIndex).amm.oracle;

		const oraclePrice =
			this.normalClient.getOracleDataForMarket(marketIndex).price;

		const market = this.normalClient.getPerpMarketAccount(marketIndex);
		const currentVaultPosition =
			this.getVaultPositionWithLPSettle(marketIndex, undefined, true)[0] ||
			this.getEmptyPosition(marketIndex);

		positionBaseSizeChange = standardizeBaseAssetAmount(
			positionBaseSizeChange,
			amm.orderStepSize
		);

		const freeCollateralChangeFromNewPosition =
			this.calculateEntriesEffectOnFreeCollateral(
				market,
				oraclePrice,
				currentVaultPosition,
				positionBaseSizeChange,
				estimatedEntryPrice,
				includeOpenOrders
			);

		freeCollateral = freeCollateral.add(freeCollateralChangeFromNewPosition);

		let freeCollateralDelta = this.calculateFreeCollateralDeltaForPerp(
			market,
			currentVaultPosition,
			positionBaseSizeChange,
			oraclePrice,
			marginCategory,
			includeOpenOrders
		);

		if (!freeCollateralDelta) {
			return new BN(-1);
		}

		const spotMarketWithSameOracle = this.normalClient
			.getSpotMarketAccounts()
			.find((market) => market.oracle.equals(oracle));
		if (spotMarketWithSameOracle) {
			const spotPosition = this.getSpotPosition(
				spotMarketWithSameOracle.marketIndex
			);
			if (spotPosition) {
				const signedTokenAmount = getSignedTokenAmount(
					getTokenAmount(
						spotPosition.scaledBalance,
						spotMarketWithSameOracle,
						spotPosition.balanceType
					),
					spotPosition.balanceType
				);

				const spotFreeCollateralDelta =
					this.calculateFreeCollateralDeltaForSpot(
						spotMarketWithSameOracle,
						signedTokenAmount,
						marginCategory
					);
				freeCollateralDelta = freeCollateralDelta.add(
					spotFreeCollateralDelta || ZERO
				);
			}
		}

		if (freeCollateralDelta.eq(ZERO)) {
			return new BN(-1);
		}

		const liqPriceDelta = freeCollateral
			.mul(QUOTE_PRECISION)
			.div(freeCollateralDelta);

		const liqPrice = oraclePrice.sub(liqPriceDelta);

		if (liqPrice.lt(ZERO)) {
			return new BN(-1);
		}

		return liqPrice;
	}

	calculateEntriesEffectOnFreeCollateral(
		market: PerpMarketAccount,
		oraclePrice: BN,
		perpPosition: VaultPosition,
		positionBaseSizeChange: BN,
		estimatedEntryPrice: BN,
		includeOpenOrders: boolean
	): BN {
		let freeCollateralChange = ZERO;

		// update free collateral to account for change in pnl from new position
		if (!estimatedEntryPrice.eq(ZERO) && !positionBaseSizeChange.eq(ZERO)) {
			const costBasis = oraclePrice
				.mul(positionBaseSizeChange.abs())
				.div(BASE_PRECISION);
			const newPositionValue = estimatedEntryPrice
				.mul(positionBaseSizeChange.abs())
				.div(BASE_PRECISION);
			if (positionBaseSizeChange.gt(ZERO)) {
				freeCollateralChange = costBasis.sub(newPositionValue);
			} else {
				freeCollateralChange = newPositionValue.sub(costBasis);
			}

			// assume worst fee tier
			const takerFeeTier =
				this.normalClient.getStateAccount().perpFeeStructure.feeTiers[0];
			const takerFee = newPositionValue
				.muln(takerFeeTier.feeNumerator)
				.divn(takerFeeTier.feeDenominator);
			freeCollateralChange = freeCollateralChange.sub(takerFee);
		}

		const calculateMarginRequirement = (perpPosition: VaultPosition) => {
			let baseAssetAmount: BN;
			let liabilityValue: BN;
			if (includeOpenOrders) {
				const { worstCaseBaseAssetAmount, worstCaseLiabilityValue } =
					calculateWorstCasePerpLiabilityValue(
						perpPosition,
						market,
						oraclePrice
					);
				baseAssetAmount = worstCaseBaseAssetAmount;
				liabilityValue = worstCaseLiabilityValue;
			} else {
				baseAssetAmount = perpPosition.baseAssetAmount;
				liabilityValue = calculatePerpLiabilityValue(
					baseAssetAmount,
					oraclePrice,
					isVariant(market.contractType, 'prediction')
				);
			}

			const marginRatio = calculateMarketMarginRatio(
				market,
				baseAssetAmount.abs(),
				'Maintenance'
			);

			return liabilityValue.mul(new BN(marginRatio)).div(MARGIN_PRECISION);
		};

		const freeCollateralConsumptionBefore =
			calculateMarginRequirement(perpPosition);

		const perpPositionAfter = Object.assign({}, perpPosition);
		perpPositionAfter.baseAssetAmount = perpPositionAfter.baseAssetAmount.add(
			positionBaseSizeChange
		);

		const freeCollateralConsumptionAfter =
			calculateMarginRequirement(perpPositionAfter);

		return freeCollateralChange.sub(
			freeCollateralConsumptionAfter.sub(freeCollateralConsumptionBefore)
		);
	}

	calculateFreeCollateralDeltaForPerp(
		market: PerpMarketAccount,
		perpPosition: VaultPosition,
		positionBaseSizeChange: BN,
		oraclePrice: BN,
		marginCategory: MarginCategory = 'Maintenance',
		includeOpenOrders = false
	): BN | undefined {
		const baseAssetAmount = includeOpenOrders
			? calculateWorstCaseBaseAssetAmount(perpPosition, market, oraclePrice)
			: perpPosition.baseAssetAmount;

		// zero if include orders == false
		const orderBaseAssetAmount = baseAssetAmount.sub(
			perpPosition.baseAssetAmount
		);

		const proposedBaseAssetAmount = baseAssetAmount.add(positionBaseSizeChange);

		const marginRatio = calculateMarketMarginRatio(
			market,
			proposedBaseAssetAmount.abs(),
			marginCategory,
			this.getUserAccount().maxMarginRatio
		);
		const marginRatioQuotePrecision = new BN(marginRatio)
			.mul(QUOTE_PRECISION)
			.div(MARGIN_PRECISION);

		if (proposedBaseAssetAmount.eq(ZERO)) {
			return undefined;
		}

		let freeCollateralDelta = ZERO;
		if (isVariant(market.contractType, 'prediction')) {
			// for prediction market, increase in pnl and margin requirement will net out for position
			// open order margin requirement will change with price though
			if (orderBaseAssetAmount.gt(ZERO)) {
				freeCollateralDelta = marginRatioQuotePrecision.neg();
			} else if (orderBaseAssetAmount.lt(ZERO)) {
				freeCollateralDelta = marginRatioQuotePrecision;
			}
		} else {
			if (proposedBaseAssetAmount.gt(ZERO)) {
				freeCollateralDelta = QUOTE_PRECISION.sub(marginRatioQuotePrecision)
					.mul(proposedBaseAssetAmount)
					.div(BASE_PRECISION);
			} else {
				freeCollateralDelta = QUOTE_PRECISION.neg()
					.sub(marginRatioQuotePrecision)
					.mul(proposedBaseAssetAmount.abs())
					.div(BASE_PRECISION);
			}

			if (!orderBaseAssetAmount.eq(ZERO)) {
				freeCollateralDelta = freeCollateralDelta.sub(
					marginRatioQuotePrecision
						.mul(orderBaseAssetAmount.abs())
						.div(BASE_PRECISION)
				);
			}
		}

		return freeCollateralDelta;
	}

	calculateFreeCollateralDeltaForSpot(
		market: SpotMarketAccount,
		signedTokenAmount: BN,
		marginCategory: MarginCategory = 'Maintenance'
	): BN {
		const tokenPrecision = new BN(Math.pow(10, market.decimals));

		if (signedTokenAmount.gt(ZERO)) {
			const assetWeight = calculateAssetWeight(
				signedTokenAmount,
				this.normalClient.getOracleDataForSpotMarket(market.marketIndex).price,
				market,
				marginCategory
			);

			return QUOTE_PRECISION.mul(assetWeight)
				.div(SPOT_MARKET_WEIGHT_PRECISION)
				.mul(signedTokenAmount)
				.div(tokenPrecision);
		} else {
			const liabilityWeight = calculateLiabilityWeight(
				signedTokenAmount.abs(),
				market,
				marginCategory
			);

			return QUOTE_PRECISION.neg()
				.mul(liabilityWeight)
				.div(SPOT_MARKET_WEIGHT_PRECISION)
				.mul(signedTokenAmount.abs())
				.div(tokenPrecision);
		}
	}

	/**
	 * Calculates the estimated liquidation price for a position after closing a quote amount of the position.
	 * @param positionMarketIndex
	 * @param closeQuoteAmount
	 * @returns : Precision PRICE_PRECISION
	 */
	public liquidationPriceAfterClose(
		positionMarketIndex: number,
		closeQuoteAmount: BN,
		estimatedEntryPrice: BN = ZERO
	): BN {
		const currentPosition =
			this.getVaultPositionWithLPSettle(
				positionMarketIndex,
				undefined,
				true
			)[0] || this.getEmptyPosition(positionMarketIndex);

		const closeBaseAmount = currentPosition.baseAssetAmount
			.mul(closeQuoteAmount)
			.div(currentPosition.quoteAssetAmount.abs())
			.add(
				currentPosition.baseAssetAmount
					.mul(closeQuoteAmount)
					.mod(currentPosition.quoteAssetAmount.abs())
			)
			.neg();

		return this.liquidationPrice(
			positionMarketIndex,
			closeBaseAmount,
			estimatedEntryPrice
		);
	}

	public getMarginUSDCRequiredForTrade(
		targetMarketIndex: number,
		baseSize: BN
	): BN {
		return calculateMarginUSDCRequiredForTrade(
			this.normalClient,
			targetMarketIndex,
			baseSize,
			this.getUserAccount().maxMarginRatio
		);
	}

	public getCollateralDepositRequiredForTrade(
		targetMarketIndex: number,
		baseSize: BN,
		collateralIndex: number
	): BN {
		return calculateCollateralDepositRequiredForTrade(
			this.normalClient,
			targetMarketIndex,
			baseSize,
			collateralIndex,
			this.getUserAccount().maxMarginRatio
		);
	}

	/**
	 * Get the maximum trade size for a given market, taking into account the user's current leverage, positions, collateral, etc.
	 *
	 * To Calculate Max Quote Available:
	 *
	 * Case 1: SameSide
	 * 	=> Remaining quote to get to maxLeverage
	 *
	 * Case 2: NOT SameSide && currentLeverage <= maxLeverage
	 * 	=> Current opposite position x2 + remaining to get to maxLeverage
	 *
	 * Case 3: NOT SameSide && currentLeverage > maxLeverage && otherPositions - currentPosition > maxLeverage
	 * 	=> strictly reduce current position size
	 *
	 * Case 4: NOT SameSide && currentLeverage > maxLeverage && otherPositions - currentPosition < maxLeverage
	 * 	=> current position + remaining to get to maxLeverage
	 *
	 * @param targetMarketIndex
	 * @param tradeSide
	 * @param isLp
	 * @returns { tradeSize: BN, oppositeSideTradeSize: BN} : Precision QUOTE_PRECISION
	 */
	public getMaxTradeSizeUSDCForPerp(
		targetMarketIndex: number,
		tradeSide: PositionDirection,
		isLp = false
	): { tradeSize: BN; oppositeSideTradeSize: BN } {
		let tradeSize = ZERO;
		let oppositeSideTradeSize = ZERO;
		const currentPosition =
			this.getVaultPositionWithLPSettle(
				targetMarketIndex,
				undefined,
				true
			)[0] || this.getEmptyPosition(targetMarketIndex);

		const targetSide = isVariant(tradeSide, 'short') ? 'short' : 'long';

		const currentPositionSide = currentPosition?.baseAssetAmount.isNeg()
			? 'short'
			: 'long';

		const targetingSameSide = !currentPosition
			? true
			: targetSide === currentPositionSide;

		const oracleData = this.getOracleDataForMarket(targetMarketIndex);

		const marketAccount =
			this.normalClient.getPerpMarketAccount(targetMarketIndex);

		const lpBuffer = isLp
			? oracleData.price
					.mul(marketAccount.amm.orderStepSize)
					.div(AMM_RESERVE_PRECISION)
			: ZERO;

		// add any position we have on the opposite side of the current trade, because we can "flip" the size of this position without taking any extra leverage.
		const oppositeSizeLiabilityValue = targetingSameSide
			? ZERO
			: calculatePerpLiabilityValue(
					currentPosition.baseAssetAmount,
					oracleData.price,
					isVariant(marketAccount.contractType, 'prediction')
			  );

		const maxPositionSize = this.getPerpBuyingPower(
			targetMarketIndex,
			lpBuffer
		);

		if (maxPositionSize.gte(ZERO)) {
			if (oppositeSizeLiabilityValue.eq(ZERO)) {
				// case 1 : Regular trade where current total position less than max, and no opposite position to account for
				// do nothing
				tradeSize = maxPositionSize;
			} else {
				// case 2 : trade where current total position less than max, but need to account for flipping the current position over to the other side
				tradeSize = maxPositionSize.add(oppositeSizeLiabilityValue);
				oppositeSideTradeSize = oppositeSizeLiabilityValue;
			}
		} else {
			// current leverage is greater than max leverage - can only reduce position size

			if (!targetingSameSide) {
				const market =
					this.normalClient.getPerpMarketAccount(targetMarketIndex);
				const perpLiabilityValue = calculatePerpLiabilityValue(
					currentPosition.baseAssetAmount,
					oracleData.price,
					isVariant(market.contractType, 'prediction')
				);
				const totalCollateral = this.getTotalCollateral();
				const marginRequirement = this.getInitialMarginRequirement();
				const marginFreedByClosing = perpLiabilityValue
					.mul(new BN(market.marginRatioInitial))
					.div(MARGIN_PRECISION);
				const marginRequirementAfterClosing =
					marginRequirement.sub(marginFreedByClosing);

				if (marginRequirementAfterClosing.gt(totalCollateral)) {
					oppositeSideTradeSize = perpLiabilityValue;
				} else {
					const freeCollateralAfterClose = totalCollateral.sub(
						marginRequirementAfterClosing
					);

					const buyingPowerAfterClose =
						this.getPerpBuyingPowerFromFreeCollateralAndBaseAssetAmount(
							targetMarketIndex,
							freeCollateralAfterClose,
							ZERO
						);
					oppositeSideTradeSize = perpLiabilityValue;
					tradeSize = buyingPowerAfterClose;
				}
			} else {
				// do nothing if targetting same side
				tradeSize = maxPositionSize;
			}
		}

		return { tradeSize, oppositeSideTradeSize };
	}

	/**
	 * Get the maximum trade size for a given market, taking into account the user's current leverage, positions, collateral, etc.
	 *
	 * @param targetMarketIndex
	 * @param direction
	 * @param currentQuoteAssetValue
	 * @param currentSpotMarketNetValue
	 * @returns tradeSizeAllowed : Precision QUOTE_PRECISION
	 */
	public getMaxTradeSizeUSDCForSpot(
		targetMarketIndex: number,
		direction: PositionDirection,
		currentQuoteAssetValue?: BN,
		currentSpotMarketNetValue?: BN
	): BN {
		const market = this.normalClient.getSpotMarketAccount(targetMarketIndex);
		const oraclePrice =
			this.normalClient.getOracleDataForSpotMarket(targetMarketIndex).price;

		currentQuoteAssetValue = this.getSpotMarketAssetValue(
			QUOTE_SPOT_MARKET_INDEX
		);

		currentSpotMarketNetValue =
			currentSpotMarketNetValue ?? this.getSpotPositionValue(targetMarketIndex);

		let freeCollateral = this.getFreeCollateral();
		const marginRatio = calculateSpotMarketMarginRatio(
			market,
			oraclePrice,
			'Initial',
			ZERO,
			isVariant(direction, 'long')
				? SpotBalanceType.DEPOSIT
				: SpotBalanceType.BORROW,
			this.getUserAccount().maxMarginRatio
		);

		let tradeAmount = ZERO;
		if (this.getUserAccount().isMarginTradingEnabled) {
			// if the user is buying/selling and already short/long, need to account for closing out short/long
			if (isVariant(direction, 'long') && currentSpotMarketNetValue.lt(ZERO)) {
				tradeAmount = currentSpotMarketNetValue.abs();
				const marginRatio = calculateSpotMarketMarginRatio(
					market,
					oraclePrice,
					'Initial',
					this.getTokenAmount(targetMarketIndex).abs(),
					SpotBalanceType.BORROW,
					this.getUserAccount().maxMarginRatio
				);
				freeCollateral = freeCollateral.add(
					tradeAmount.mul(new BN(marginRatio)).div(MARGIN_PRECISION)
				);
			} else if (
				isVariant(direction, 'short') &&
				currentSpotMarketNetValue.gt(ZERO)
			) {
				tradeAmount = currentSpotMarketNetValue;
				const marginRatio = calculateSpotMarketMarginRatio(
					market,
					oraclePrice,
					'Initial',
					this.getTokenAmount(targetMarketIndex),
					SpotBalanceType.DEPOSIT,
					this.getUserAccount().maxMarginRatio
				);
				freeCollateral = freeCollateral.add(
					tradeAmount.mul(new BN(marginRatio)).div(MARGIN_PRECISION)
				);
			}

			tradeAmount = tradeAmount.add(
				freeCollateral.mul(MARGIN_PRECISION).div(new BN(marginRatio))
			);
		} else if (isVariant(direction, 'long')) {
			tradeAmount = BN.min(
				currentQuoteAssetValue,
				freeCollateral.mul(MARGIN_PRECISION).div(new BN(marginRatio))
			);
		} else {
			tradeAmount = BN.max(ZERO, currentSpotMarketNetValue);
		}

		return tradeAmount;
	}

	/**
	 * Calculates the max amount of token that can be swapped from inMarket to outMarket
	 * Assumes swap happens at oracle price
	 *
	 * @param inMarketIndex
	 * @param outMarketIndex
	 * @param calculateSwap function to similate in to out swa
	 * @param iterationLimit how long to run appromixation before erroring out
	 */
	public getMaxSwapAmount({
		inMarketIndex,
		outMarketIndex,
		calculateSwap,
		iterationLimit = 1000,
	}: {
		inMarketIndex: number;
		outMarketIndex: number;
		calculateSwap?: (inAmount: BN) => BN;
		iterationLimit?: number;
	}): { inAmount: BN; outAmount: BN; leverage: BN } {
		const inMarket = this.normalClient.getSpotMarketAccount(inMarketIndex);
		const outMarket = this.normalClient.getSpotMarketAccount(outMarketIndex);

		const inOraclePriceData = this.getOracleDataForSpotMarket(inMarketIndex);
		const inOraclePrice = inOraclePriceData.price;
		const outOraclePriceData = this.getOracleDataForSpotMarket(outMarketIndex);
		const outOraclePrice = outOraclePriceData.price;

		const inStrictOraclePrice = new StrictOraclePrice(inOraclePrice);
		const outStrictOraclePrice = new StrictOraclePrice(outOraclePrice);

		const inPrecision = new BN(10 ** inMarket.decimals);
		const outPrecision = new BN(10 ** outMarket.decimals);

		const inSpotPosition =
			this.getSpotPosition(inMarketIndex) ||
			this.getEmptySpotPosition(inMarketIndex);
		const outSpotPosition =
			this.getSpotPosition(outMarketIndex) ||
			this.getEmptySpotPosition(outMarketIndex);

		const freeCollateral = this.getFreeCollateral();

		const inContributionInitial =
			this.calculateSpotPositionFreeCollateralContribution(
				inSpotPosition,
				inStrictOraclePrice
			);
		const {
			totalAssetValue: inTotalAssetValueInitial,
			totalLiabilityValue: inTotalLiabilityValueInitial,
		} = this.calculateSpotPositionLeverageContribution(
			inSpotPosition,
			inStrictOraclePrice
		);
		const outContributionInitial =
			this.calculateSpotPositionFreeCollateralContribution(
				outSpotPosition,
				outStrictOraclePrice
			);
		const {
			totalAssetValue: outTotalAssetValueInitial,
			totalLiabilityValue: outTotalLiabilityValueInitial,
		} = this.calculateSpotPositionLeverageContribution(
			outSpotPosition,
			outStrictOraclePrice
		);
		const initialContribution = inContributionInitial.add(
			outContributionInitial
		);

		const { perpLiabilityValue, perpPnl, spotAssetValue, spotLiabilityValue } =
			this.getLeverageComponents();

		if (!calculateSwap) {
			calculateSwap = (inSwap: BN) => {
				return inSwap
					.mul(outPrecision)
					.mul(inOraclePrice)
					.div(outOraclePrice)
					.div(inPrecision);
			};
		}

		let inSwap = ZERO;
		let outSwap = ZERO;
		const inTokenAmount = this.getTokenAmount(inMarketIndex);
		const outTokenAmount = this.getTokenAmount(outMarketIndex);

		const outSaferThanIn =
			// selling asset to close borrow
			(inTokenAmount.gt(ZERO) && outTokenAmount.lt(ZERO)) ||
			// buying asset with higher initial asset weight
			inMarket.initialAssetWeight < outMarket.initialAssetWeight;

		if (freeCollateral.lt(ONE)) {
			if (outSaferThanIn && inTokenAmount.gt(ZERO)) {
				inSwap = inTokenAmount;
				outSwap = calculateSwap(inSwap);
			}
		} else {
			let minSwap = ZERO;
			let maxSwap = BN.max(
				freeCollateral.mul(inPrecision).mul(new BN(100)).div(inOraclePrice), // 100x current free collateral
				inTokenAmount.abs().mul(new BN(10)) // 10x current position
			);
			inSwap = maxSwap.div(TWO);
			const error = freeCollateral.div(new BN(10000));

			let i = 0;
			let freeCollateralAfter = freeCollateral;
			while (freeCollateralAfter.gt(error) || freeCollateralAfter.isNeg()) {
				outSwap = calculateSwap(inSwap);

				const inPositionAfter = this.cloneAndUpdateSpotPosition(
					inSpotPosition,
					inSwap.neg(),
					inMarket
				);
				const outPositionAfter = this.cloneAndUpdateSpotPosition(
					outSpotPosition,
					outSwap,
					outMarket
				);

				const inContributionAfter =
					this.calculateSpotPositionFreeCollateralContribution(
						inPositionAfter,
						inStrictOraclePrice
					);
				const outContributionAfter =
					this.calculateSpotPositionFreeCollateralContribution(
						outPositionAfter,
						outStrictOraclePrice
					);

				const contributionAfter = inContributionAfter.add(outContributionAfter);

				const contributionDelta = contributionAfter.sub(initialContribution);

				freeCollateralAfter = freeCollateral.add(contributionDelta);

				if (freeCollateralAfter.gt(error)) {
					minSwap = inSwap;
					inSwap = minSwap.add(maxSwap).div(TWO);
				} else if (freeCollateralAfter.isNeg()) {
					maxSwap = inSwap;
					inSwap = minSwap.add(maxSwap).div(TWO);
				}

				if (i++ > iterationLimit) {
					console.log('getMaxSwapAmount iteration limit reached');
					break;
				}
			}
		}

		const inPositionAfter = this.cloneAndUpdateSpotPosition(
			inSpotPosition,
			inSwap.neg(),
			inMarket
		);
		const outPositionAfter = this.cloneAndUpdateSpotPosition(
			outSpotPosition,
			outSwap,
			outMarket
		);

		const {
			totalAssetValue: inTotalAssetValueAfter,
			totalLiabilityValue: inTotalLiabilityValueAfter,
		} = this.calculateSpotPositionLeverageContribution(
			inPositionAfter,
			inStrictOraclePrice
		);

		const {
			totalAssetValue: outTotalAssetValueAfter,
			totalLiabilityValue: outTotalLiabilityValueAfter,
		} = this.calculateSpotPositionLeverageContribution(
			outPositionAfter,
			outStrictOraclePrice
		);

		const spotAssetValueDelta = inTotalAssetValueAfter
			.add(outTotalAssetValueAfter)
			.sub(inTotalAssetValueInitial)
			.sub(outTotalAssetValueInitial);
		const spotLiabilityValueDelta = inTotalLiabilityValueAfter
			.add(outTotalLiabilityValueAfter)
			.sub(inTotalLiabilityValueInitial)
			.sub(outTotalLiabilityValueInitial);

		const spotAssetValueAfter = spotAssetValue.add(spotAssetValueDelta);
		const spotLiabilityValueAfter = spotLiabilityValue.add(
			spotLiabilityValueDelta
		);

		const leverage = this.calculateLeverageFromComponents({
			perpLiabilityValue,
			perpPnl,
			spotAssetValue: spotAssetValueAfter,
			spotLiabilityValue: spotLiabilityValueAfter,
		});

		return { inAmount: inSwap, outAmount: outSwap, leverage };
	}

	public cloneAndUpdateSpotPosition(
		position: SpotPosition,
		tokenAmount: BN,
		market: SpotMarketAccount
	): SpotPosition {
		const clonedPosition = Object.assign({}, position);
		if (tokenAmount.eq(ZERO)) {
			return clonedPosition;
		}

		const preTokenAmount = getSignedTokenAmount(
			getTokenAmount(position.scaledBalance, market, position.balanceType),
			position.balanceType
		);

		if (sigNum(preTokenAmount).eq(sigNum(tokenAmount))) {
			const scaledBalanceDelta = getBalance(
				tokenAmount.abs(),
				market,
				position.balanceType
			);
			clonedPosition.scaledBalance =
				clonedPosition.scaledBalance.add(scaledBalanceDelta);
			return clonedPosition;
		}

		const updateDirection = tokenAmount.isNeg()
			? SpotBalanceType.BORROW
			: SpotBalanceType.DEPOSIT;

		if (tokenAmount.abs().gte(preTokenAmount.abs())) {
			clonedPosition.scaledBalance = getBalance(
				tokenAmount.abs().sub(preTokenAmount.abs()),
				market,
				updateDirection
			);
			clonedPosition.balanceType = updateDirection;
		} else {
			const scaledBalanceDelta = getBalance(
				tokenAmount.abs(),
				market,
				position.balanceType
			);

			clonedPosition.scaledBalance =
				clonedPosition.scaledBalance.sub(scaledBalanceDelta);
		}
		return clonedPosition;
	}

	calculateSpotPositionFreeCollateralContribution(
		spotPosition: SpotPosition,
		strictOraclePrice: StrictOraclePrice
	): BN {
		const marginCategory = 'Initial';

		const spotMarketAccount: SpotMarketAccount =
			this.normalClient.getSpotMarketAccount(spotPosition.marketIndex);

		const { freeCollateralContribution } = getWorstCaseTokenAmounts(
			spotPosition,
			spotMarketAccount,
			strictOraclePrice,
			marginCategory,
			this.getUserAccount().maxMarginRatio
		);

		return freeCollateralContribution;
	}

	calculateSpotPositionLeverageContribution(
		spotPosition: SpotPosition,
		strictOraclePrice: StrictOraclePrice
	): {
		totalAssetValue: BN;
		totalLiabilityValue: BN;
	} {
		let totalAssetValue = ZERO;
		let totalLiabilityValue = ZERO;

		const spotMarketAccount: SpotMarketAccount =
			this.normalClient.getSpotMarketAccount(spotPosition.marketIndex);

		const { tokenValue, ordersValue } = getWorstCaseTokenAmounts(
			spotPosition,
			spotMarketAccount,
			strictOraclePrice,
			'Initial',
			this.getUserAccount().maxMarginRatio
		);

		if (tokenValue.gte(ZERO)) {
			totalAssetValue = tokenValue;
		} else {
			totalLiabilityValue = tokenValue.abs();
		}

		if (ordersValue.gt(ZERO)) {
			totalAssetValue = totalAssetValue.add(ordersValue);
		} else {
			totalLiabilityValue = totalLiabilityValue.add(ordersValue.abs());
		}

		return {
			totalAssetValue,
			totalLiabilityValue,
		};
	}

	/**
	 * Estimates what the user leverage will be after swap
	 * @param inMarketIndex
	 * @param outMarketIndex
	 * @param inAmount
	 * @param outAmount
	 */
	public accountLeverageAfterSwap({
		inMarketIndex,
		outMarketIndex,
		inAmount,
		outAmount,
	}: {
		inMarketIndex: number;
		outMarketIndex: number;
		inAmount: BN;
		outAmount: BN;
	}): BN {
		const inMarket = this.normalClient.getSpotMarketAccount(inMarketIndex);
		const outMarket = this.normalClient.getSpotMarketAccount(outMarketIndex);

		const inOraclePriceData = this.getOracleDataForSpotMarket(inMarketIndex);
		const inOraclePrice = inOraclePriceData.price;
		const outOraclePriceData = this.getOracleDataForSpotMarket(outMarketIndex);
		const outOraclePrice = outOraclePriceData.price;
		const inStrictOraclePrice = new StrictOraclePrice(inOraclePrice);
		const outStrictOraclePrice = new StrictOraclePrice(outOraclePrice);

		const inSpotPosition =
			this.getSpotPosition(inMarketIndex) ||
			this.getEmptySpotPosition(inMarketIndex);
		const outSpotPosition =
			this.getSpotPosition(outMarketIndex) ||
			this.getEmptySpotPosition(outMarketIndex);

		const {
			totalAssetValue: inTotalAssetValueInitial,
			totalLiabilityValue: inTotalLiabilityValueInitial,
		} = this.calculateSpotPositionLeverageContribution(
			inSpotPosition,
			inStrictOraclePrice
		);
		const {
			totalAssetValue: outTotalAssetValueInitial,
			totalLiabilityValue: outTotalLiabilityValueInitial,
		} = this.calculateSpotPositionLeverageContribution(
			outSpotPosition,
			outStrictOraclePrice
		);

		const { perpLiabilityValue, perpPnl, spotAssetValue, spotLiabilityValue } =
			this.getLeverageComponents();

		const inPositionAfter = this.cloneAndUpdateSpotPosition(
			inSpotPosition,
			inAmount.abs().neg(),
			inMarket
		);
		const outPositionAfter = this.cloneAndUpdateSpotPosition(
			outSpotPosition,
			outAmount.abs(),
			outMarket
		);

		const {
			totalAssetValue: inTotalAssetValueAfter,
			totalLiabilityValue: inTotalLiabilityValueAfter,
		} = this.calculateSpotPositionLeverageContribution(
			inPositionAfter,
			inStrictOraclePrice
		);

		const {
			totalAssetValue: outTotalAssetValueAfter,
			totalLiabilityValue: outTotalLiabilityValueAfter,
		} = this.calculateSpotPositionLeverageContribution(
			outPositionAfter,
			outStrictOraclePrice
		);

		const spotAssetValueDelta = inTotalAssetValueAfter
			.add(outTotalAssetValueAfter)
			.sub(inTotalAssetValueInitial)
			.sub(outTotalAssetValueInitial);
		const spotLiabilityValueDelta = inTotalLiabilityValueAfter
			.add(outTotalLiabilityValueAfter)
			.sub(inTotalLiabilityValueInitial)
			.sub(outTotalLiabilityValueInitial);

		const spotAssetValueAfter = spotAssetValue.add(spotAssetValueDelta);
		const spotLiabilityValueAfter = spotLiabilityValue.add(
			spotLiabilityValueDelta
		);

		return this.calculateLeverageFromComponents({
			perpLiabilityValue,
			perpPnl,
			spotAssetValue: spotAssetValueAfter,
			spotLiabilityValue: spotLiabilityValueAfter,
		});
	}

	// TODO - should this take the price impact of the trade into account for strict accuracy?

	/**
	 * Returns the leverage ratio for the account after adding (or subtracting) the given quote size to the given position
	 * @param targetMarketIndex
	 * @param: targetMarketType
	 * @param tradeQuoteAmount
	 * @param tradeSide
	 * @param includeOpenOrders
	 * @returns leverageRatio : Precision TEN_THOUSAND
	 */
	public accountLeverageRatioAfterTrade(
		targetMarketIndex: number,
		targetMarketType: MarketType,
		tradeQuoteAmount: BN,
		tradeSide: PositionDirection,
		includeOpenOrders = true
	): BN {
		const tradeIsPerp = isVariant(targetMarketType, 'perp');

		if (!tradeIsPerp) {
			// calculate new asset/liability values for base and quote market to find new account leverage
			const totalLiabilityValue = this.getTotalLiabilityValue();
			const totalAssetValue = this.getTotalAssetValue();
			const spotLiabilityValue = this.getSpotMarketLiabilityValue(
				undefined,
				undefined,
				undefined,
				includeOpenOrders
			);

			const currentQuoteAssetValue = this.getSpotMarketAssetValue(
				QUOTE_SPOT_MARKET_INDEX,
				undefined,
				includeOpenOrders
			);
			const currentQuoteLiabilityValue = this.getSpotMarketLiabilityValue(
				QUOTE_SPOT_MARKET_INDEX,
				undefined,
				undefined,
				includeOpenOrders
			);
			const currentQuoteValue = currentQuoteAssetValue.sub(
				currentQuoteLiabilityValue
			);

			const currentSpotMarketAssetValue = this.getSpotMarketAssetValue(
				targetMarketIndex,
				undefined,
				includeOpenOrders
			);
			const currentSpotMarketLiabilityValue = this.getSpotMarketLiabilityValue(
				targetMarketIndex,
				undefined,
				undefined,
				includeOpenOrders
			);
			const currentSpotMarketNetValue = currentSpotMarketAssetValue.sub(
				currentSpotMarketLiabilityValue
			);

			let assetValueToAdd = ZERO;
			let liabilityValueToAdd = ZERO;

			const newQuoteNetValue =
				tradeSide == PositionDirection.SHORT
					? currentQuoteValue.add(tradeQuoteAmount)
					: currentQuoteValue.sub(tradeQuoteAmount);
			const newQuoteAssetValue = BN.max(newQuoteNetValue, ZERO);
			const newQuoteLiabilityValue = BN.min(newQuoteNetValue, ZERO).abs();

			assetValueToAdd = assetValueToAdd.add(
				newQuoteAssetValue.sub(currentQuoteAssetValue)
			);
			liabilityValueToAdd = liabilityValueToAdd.add(
				newQuoteLiabilityValue.sub(currentQuoteLiabilityValue)
			);

			const newSpotMarketNetValue =
				tradeSide == PositionDirection.LONG
					? currentSpotMarketNetValue.add(tradeQuoteAmount)
					: currentSpotMarketNetValue.sub(tradeQuoteAmount);
			const newSpotMarketAssetValue = BN.max(newSpotMarketNetValue, ZERO);
			const newSpotMarketLiabilityValue = BN.min(
				newSpotMarketNetValue,
				ZERO
			).abs();

			assetValueToAdd = assetValueToAdd.add(
				newSpotMarketAssetValue.sub(currentSpotMarketAssetValue)
			);
			liabilityValueToAdd = liabilityValueToAdd.add(
				newSpotMarketLiabilityValue.sub(currentSpotMarketLiabilityValue)
			);

			const totalAssetValueAfterTrade = totalAssetValue.add(assetValueToAdd);
			const totalSpotLiabilityValueAfterTrade =
				spotLiabilityValue.add(liabilityValueToAdd);

			const totalLiabilityValueAfterTrade =
				totalLiabilityValue.add(liabilityValueToAdd);

			const netAssetValueAfterTrade = totalAssetValueAfterTrade.sub(
				totalSpotLiabilityValueAfterTrade
			);

			if (netAssetValueAfterTrade.eq(ZERO)) {
				return ZERO;
			}

			const newLeverage = totalLiabilityValueAfterTrade
				.mul(TEN_THOUSAND)
				.div(netAssetValueAfterTrade);

			return newLeverage;
		}

		const currentPosition =
			this.getVaultPositionWithLPSettle(targetMarketIndex)[0] ||
			this.getEmptyPosition(targetMarketIndex);

		const perpMarket =
			this.normalClient.getPerpMarketAccount(targetMarketIndex);
		const oracleData = this.getOracleDataForMarket(targetMarketIndex);

		let {
			// eslint-disable-next-line prefer-const
			worstCaseBaseAssetAmount: worstCaseBase,
			worstCaseLiabilityValue: currentPositionQuoteAmount,
		} = calculateWorstCasePerpLiabilityValue(
			currentPosition,
			perpMarket,
			oracleData.price
		);

		// current side is short if position base asset amount is negative OR there is no position open but open orders are short
		const currentSide =
			currentPosition.baseAssetAmount.isNeg() ||
			(currentPosition.baseAssetAmount.eq(ZERO) && worstCaseBase.isNeg())
				? PositionDirection.SHORT
				: PositionDirection.LONG;

		if (currentSide === PositionDirection.SHORT)
			currentPositionQuoteAmount = currentPositionQuoteAmount.neg();

		if (tradeSide === PositionDirection.SHORT)
			tradeQuoteAmount = tradeQuoteAmount.neg();

		const currentVaultPositionAfterTrade = currentPositionQuoteAmount
			.add(tradeQuoteAmount)
			.abs();

		const totalPositionAfterTradeExcludingTargetMarket =
			this.getTotalVaultPositionValueExcludingMarket(
				targetMarketIndex,
				undefined,
				undefined,
				includeOpenOrders
			);

		const totalAssetValue = this.getTotalAssetValue();

		const totalVaultPositionLiability = currentVaultPositionAfterTrade
			.add(totalPositionAfterTradeExcludingTargetMarket)
			.abs();

		const totalSpotLiability = this.getSpotMarketLiabilityValue(
			undefined,
			undefined,
			undefined,
			includeOpenOrders
		);

		const totalLiabilitiesAfterTrade =
			totalVaultPositionLiability.add(totalSpotLiability);

		const netAssetValue = totalAssetValue.sub(totalSpotLiability);

		if (netAssetValue.eq(ZERO)) {
			return ZERO;
		}

		const newLeverage = totalLiabilitiesAfterTrade
			.mul(TEN_THOUSAND)
			.div(netAssetValue);

		return newLeverage;
	}

	public getUserFeeTier(marketType: MarketType, now?: BN) {
		const state = this.normalClient.getStateAccount();

		let feeTierIndex = 0;
		if (isVariant(marketType, 'perp')) {
			const userStatsAccount: UserStatsAccount = this.normalClient
				.getUserStats()
				.getAccount();

			const total30dVolume = getUser30dRollingVolumeEstimate(
				userStatsAccount,
				now
			);

			const stakedQuoteAssetAmount = userStatsAccount.ifStakedQuoteAssetAmount;
			const volumeTiers = [
				new BN(100_000_000).mul(QUOTE_PRECISION),
				new BN(50_000_000).mul(QUOTE_PRECISION),
				new BN(10_000_000).mul(QUOTE_PRECISION),
				new BN(5_000_000).mul(QUOTE_PRECISION),
				new BN(1_000_000).mul(QUOTE_PRECISION),
			];
			const stakedTiers = [
				new BN(10000).mul(QUOTE_PRECISION),
				new BN(5000).mul(QUOTE_PRECISION),
				new BN(2000).mul(QUOTE_PRECISION),
				new BN(1000).mul(QUOTE_PRECISION),
				new BN(500).mul(QUOTE_PRECISION),
			];

			for (let i = 0; i < volumeTiers.length; i++) {
				if (
					total30dVolume.gte(volumeTiers[i]) ||
					stakedQuoteAssetAmount.gte(stakedTiers[i])
				) {
					feeTierIndex = 5 - i;
					break;
				}
			}

			return state.perpFeeStructure.feeTiers[feeTierIndex];
		}

		return state.spotFeeStructure.feeTiers[feeTierIndex];
	}

	/**
	 * Calculates how much perp fee will be taken for a given sized trade
	 * @param quoteAmount
	 * @returns feeForQuote : Precision QUOTE_PRECISION
	 */
	public calculateFeeForQuoteAmount(quoteAmount: BN, marketIndex?: number): BN {
		if (marketIndex !== undefined) {
			const takerFeeMultiplier = this.normalClient.getMarketFees(
				MarketType.PERP,
				marketIndex,
				this
			).takerFee;
			const feeAmountNum =
				BigNum.from(quoteAmount, QUOTE_PRECISION_EXP).toNum() *
				takerFeeMultiplier;
			return BigNum.fromPrint(feeAmountNum.toString(), QUOTE_PRECISION_EXP).val;
		} else {
			const feeTier = this.getUserFeeTier(MarketType.PERP);
			return quoteAmount
				.mul(new BN(feeTier.feeNumerator))
				.div(new BN(feeTier.feeDenominator));
		}
	}

	/**
	 * Calculates a user's max withdrawal amounts for a spot market. If reduceOnly is true,
	 * it will return the max withdrawal amount without opening a liability for the user
	 * @param marketIndex
	 * @returns withdrawalLimit : Precision is the token precision for the chosen SpotMarket
	 */
	public getWithdrawalLimit(marketIndex: number, reduceOnly?: boolean): BN {
		const nowTs = new BN(Math.floor(Date.now() / 1000));
		const spotMarket = this.normalClient.getSpotMarketAccount(marketIndex);

		// eslint-disable-next-line prefer-const
		let { borrowLimit, withdrawLimit } = calculateWithdrawLimit(
			spotMarket,
			nowTs
		);

		const freeCollateral = this.getFreeCollateral();
		const initialMarginRequirement = this.getInitialMarginRequirement();
		const oracleData = this.getOracleDataForSpotMarket(marketIndex);
		const precisionIncrease = TEN.pow(new BN(spotMarket.decimals - 6));

		const { canBypass, depositAmount: userDepositAmount } =
			this.canBypassWithdrawLimits(marketIndex);
		if (canBypass) {
			withdrawLimit = BN.max(withdrawLimit, userDepositAmount);
		}

		const assetWeight = calculateAssetWeight(
			userDepositAmount,
			oracleData.price,
			spotMarket,
			'Initial'
		);

		let amountWithdrawable;
		if (assetWeight.eq(ZERO)) {
			amountWithdrawable = userDepositAmount;
		} else if (initialMarginRequirement.eq(ZERO)) {
			amountWithdrawable = userDepositAmount;
		} else {
			amountWithdrawable = divCeil(
				divCeil(freeCollateral.mul(MARGIN_PRECISION), assetWeight).mul(
					PRICE_PRECISION
				),
				oracleData.price
			).mul(precisionIncrease);
		}

		const maxWithdrawValue = BN.min(
			BN.min(amountWithdrawable, userDepositAmount),
			withdrawLimit.abs()
		);

		if (reduceOnly) {
			return BN.max(maxWithdrawValue, ZERO);
		} else {
			const weightedAssetValue = this.getSpotMarketAssetValue(
				marketIndex,
				'Initial',
				false
			);

			const freeCollatAfterWithdraw = userDepositAmount.gt(ZERO)
				? freeCollateral.sub(weightedAssetValue)
				: freeCollateral;

			const maxLiabilityAllowed = freeCollatAfterWithdraw
				.mul(MARGIN_PRECISION)
				.div(new BN(spotMarket.initialLiabilityWeight))
				.mul(PRICE_PRECISION)
				.div(oracleData.price)
				.mul(precisionIncrease);

			const maxBorrowValue = BN.min(
				maxWithdrawValue.add(maxLiabilityAllowed),
				borrowLimit.abs()
			);

			return BN.max(maxBorrowValue, ZERO);
		}
	}

	public canBypassWithdrawLimits(marketIndex: number): {
		canBypass: boolean;
		netDeposits: BN;
		depositAmount: BN;
		maxDepositAmount: BN;
	} {
		const spotMarket = this.normalClient.getSpotMarketAccount(marketIndex);
		const maxDepositAmount = spotMarket.withdrawGuardThreshold.div(new BN(10));
		const position = this.getSpotPosition(marketIndex);

		const netDeposits = this.getUserAccount().totalDeposits.sub(
			this.getUserAccount().totalWithdraws
		);

		if (!position) {
			return {
				canBypass: false,
				maxDepositAmount,
				depositAmount: ZERO,
				netDeposits,
			};
		}

		if (isVariant(position.balanceType, 'borrow')) {
			return {
				canBypass: false,
				maxDepositAmount,
				netDeposits,
				depositAmount: ZERO,
			};
		}

		const depositAmount = getTokenAmount(
			position.scaledBalance,
			spotMarket,
			SpotBalanceType.DEPOSIT
		);

		if (netDeposits.lt(ZERO)) {
			return {
				canBypass: false,
				maxDepositAmount,
				depositAmount,
				netDeposits,
			};
		}

		return {
			canBypass: depositAmount.lt(maxDepositAmount),
			maxDepositAmount,
			netDeposits,
			depositAmount,
		};
	}

	public canMakeIdle(slot: BN): boolean {
		const userAccount = this.getUserAccount();
		if (userAccount.idle) {
			return false;
		}

		const { totalAssetValue, totalLiabilityValue } =
			this.getSpotMarketAssetAndLiabilityValue();
		const equity = totalAssetValue.sub(totalLiabilityValue);

		let slotsBeforeIdle: BN;
		if (equity.lt(QUOTE_PRECISION.muln(1000))) {
			slotsBeforeIdle = new BN(9000); // 1 hour
		} else {
			slotsBeforeIdle = new BN(1512000); // 1 week
		}

		const userLastActiveSlot = userAccount.lastActiveSlot;
		const slotsSinceLastActive = slot.sub(userLastActiveSlot);
		if (slotsSinceLastActive.lt(slotsBeforeIdle)) {
			return false;
		}

		if (this.isBeingLiquidated()) {
			return false;
		}

		for (const perpPosition of userAccount.vaultPositions) {
			if (!positionIsAvailable(perpPosition)) {
				return false;
			}
		}

		for (const spotPosition of userAccount.spotPositions) {
			if (
				isVariant(spotPosition.balanceType, 'borrow') &&
				spotPosition.scaledBalance.gt(ZERO)
			) {
				return false;
			}

			if (spotPosition.openOrders !== 0) {
				return false;
			}
		}

		for (const order of userAccount.orders) {
			if (!isVariant(order.status, 'init')) {
				return false;
			}
		}

		return true;
	}

	public getSafestTiers(): { perpTier: number; spotTier: number } {
		let safestPerpTier = 4;
		let safestSpotTier = 4;

		for (const perpPosition of this.getActiveVaultPositions()) {
			safestPerpTier = Math.min(
				safestPerpTier,
				getPerpMarketTierNumber(
					this.normalClient.getPerpMarketAccount(perpPosition.marketIndex)
				)
			);
		}

		for (const spotPosition of this.getActiveSpotPositions()) {
			if (isVariant(spotPosition.balanceType, 'deposit')) {
				continue;
			}

			safestSpotTier = Math.min(
				safestSpotTier,
				getSpotMarketTierNumber(
					this.normalClient.getSpotMarketAccount(spotPosition.marketIndex)
				)
			);
		}

		return {
			perpTier: safestPerpTier,
			spotTier: safestSpotTier,
		};
	}

	public getVaultPositionHealth({
		marginCategory,
		perpPosition,
		oraclePriceData,
		quoteOraclePriceData,
	}: {
		marginCategory: MarginCategory;
		perpPosition: VaultPosition;
		oraclePriceData?: OraclePriceData;
		quoteOraclePriceData?: OraclePriceData;
	}): HealthComponent {
		const settledLpPosition = this.getVaultPositionWithLPSettle(
			perpPosition.marketIndex,
			perpPosition
		)[0];
		const perpMarket = this.normalClient.getPerpMarketAccount(
			perpPosition.marketIndex
		);
		const _oraclePriceData =
			oraclePriceData ||
			this.normalClient.getOracleDataForMarket(perpMarket.marketIndex);
		const oraclePrice = _oraclePriceData.price;
		const {
			worstCaseBaseAssetAmount: worstCaseBaseAmount,
			worstCaseLiabilityValue,
		} = calculateWorstCasePerpLiabilityValue(
			settledLpPosition,
			perpMarket,
			oraclePrice
		);

		const marginRatio = new BN(
			calculateMarketMarginRatio(
				perpMarket,
				worstCaseBaseAmount.abs(),
				marginCategory,
				this.getUserAccount().maxMarginRatio
			)
		);

		const _quoteOraclePriceData =
			quoteOraclePriceData ||
			this.normalClient.getOracleDataForSpotMarket(QUOTE_SPOT_MARKET_INDEX);

		let marginRequirement = worstCaseLiabilityValue
			.mul(_quoteOraclePriceData.price)
			.div(PRICE_PRECISION)
			.mul(marginRatio)
			.div(MARGIN_PRECISION);

		marginRequirement = marginRequirement.add(
			new BN(perpPosition.openOrders).mul(OPEN_ORDER_MARGIN_REQUIREMENT)
		);

		if (perpPosition.lpShares.gt(ZERO)) {
			marginRequirement = marginRequirement.add(
				BN.max(
					QUOTE_PRECISION,
					oraclePrice
						.mul(perpamm.orderStepSize)
						.mul(QUOTE_PRECISION)
						.div(AMM_RESERVE_PRECISION)
						.div(PRICE_PRECISION)
				)
			);
		}

		return {
			marketIndex: perpMarket.marketIndex,
			size: worstCaseBaseAmount,
			value: worstCaseLiabilityValue,
			weight: marginRatio,
			weightedValue: marginRequirement,
		};
	}

	public getHealthComponents({
		marginCategory,
	}: {
		marginCategory: MarginCategory;
	}): HealthComponents {
		const healthComponents: HealthComponents = {
			deposits: [],
			borrows: [],
			vaultPositions: [],
			perpPnl: [],
		};

		for (const perpPosition of this.getActiveVaultPositions()) {
			const perpMarket = this.normalClient.getPerpMarketAccount(
				perpPosition.marketIndex
			);

			const oraclePriceData = this.normalClient.getOracleDataForMarket(
				perpMarket.marketIndex
			);

			const quoteOraclePriceData = this.normalClient.getOracleDataForSpotMarket(
				QUOTE_SPOT_MARKET_INDEX
			);

			healthComponents.vaultPositions.push(
				this.getVaultPositionHealth({
					marginCategory,
					perpPosition,
					oraclePriceData,
					quoteOraclePriceData,
				})
			);

			const quoteSpotMarket = this.normalClient.getSpotMarketAccount(
				perpMarket.quoteSpotMarketIndex
			);

			const settledVaultPosition = this.getVaultPositionWithLPSettle(
				perpPosition.marketIndex,
				perpPosition
			)[0];

			const positionUnrealizedPnl = calculatePositionPNL(
				perpMarket,
				settledVaultPosition,
				true,
				oraclePriceData
			);

			let pnlWeight;
			if (positionUnrealizedPnl.gt(ZERO)) {
				pnlWeight = calculateUnrealizedAssetWeight(
					perpMarket,
					quoteSpotMarket,
					positionUnrealizedPnl,
					marginCategory,
					oraclePriceData
				);
			} else {
				pnlWeight = SPOT_MARKET_WEIGHT_PRECISION;
			}

			const pnlValue = positionUnrealizedPnl
				.mul(quoteOraclePriceData.price)
				.div(PRICE_PRECISION);

			const wegithedPnlValue = pnlValue
				.mul(pnlWeight)
				.div(SPOT_MARKET_WEIGHT_PRECISION);

			healthComponents.perpPnl.push({
				marketIndex: perpMarket.marketIndex,
				size: positionUnrealizedPnl,
				value: pnlValue,
				weight: pnlWeight,
				weightedValue: wegithedPnlValue,
			});
		}

		let netQuoteValue = ZERO;
		for (const spotPosition of this.getActiveSpotPositions()) {
			const spotMarketAccount: SpotMarketAccount =
				this.normalClient.getSpotMarketAccount(spotPosition.marketIndex);

			const oraclePriceData = this.getOracleDataForSpotMarket(
				spotPosition.marketIndex
			);

			const strictOraclePrice = new StrictOraclePrice(oraclePriceData.price);

			if (spotPosition.marketIndex === QUOTE_SPOT_MARKET_INDEX) {
				const tokenAmount = getSignedTokenAmount(
					getTokenAmount(
						spotPosition.scaledBalance,
						spotMarketAccount,
						spotPosition.balanceType
					),
					spotPosition.balanceType
				);

				netQuoteValue = netQuoteValue.add(tokenAmount);
				continue;
			}

			const {
				tokenAmount: worstCaseTokenAmount,
				tokenValue: tokenValue,
				weight,
				weightedTokenValue: weightedTokenValue,
				ordersValue: ordersValue,
			} = getWorstCaseTokenAmounts(
				spotPosition,
				spotMarketAccount,
				strictOraclePrice,
				marginCategory,
				this.getUserAccount().maxMarginRatio
			);

			netQuoteValue = netQuoteValue.add(ordersValue);

			const baseAssetValue = tokenValue.abs();
			const weightedValue = weightedTokenValue.abs();

			if (weightedTokenValue.lt(ZERO)) {
				healthComponents.borrows.push({
					marketIndex: spotMarketAccount.marketIndex,
					size: worstCaseTokenAmount,
					value: baseAssetValue,
					weight: weight,
					weightedValue: weightedValue,
				});
			} else {
				healthComponents.deposits.push({
					marketIndex: spotMarketAccount.marketIndex,
					size: worstCaseTokenAmount,
					value: baseAssetValue,
					weight: weight,
					weightedValue: weightedValue,
				});
			}
		}

		if (!netQuoteValue.eq(ZERO)) {
			const spotMarketAccount = this.normalClient.getQuoteSpotMarketAccount();
			const oraclePriceData = this.getOracleDataForSpotMarket(
				QUOTE_SPOT_MARKET_INDEX
			);

			const baseAssetValue = getTokenValue(
				netQuoteValue,
				spotMarketAccount.decimals,
				oraclePriceData
			);

			const { weight, weightedTokenValue } = calculateWeightedTokenValue(
				netQuoteValue,
				baseAssetValue,
				oraclePriceData.price,
				spotMarketAccount,
				marginCategory,
				this.getUserAccount().maxMarginRatio
			);

			if (netQuoteValue.lt(ZERO)) {
				healthComponents.borrows.push({
					marketIndex: spotMarketAccount.marketIndex,
					size: netQuoteValue,
					value: baseAssetValue.abs(),
					weight: weight,
					weightedValue: weightedTokenValue.abs(),
				});
			} else {
				healthComponents.deposits.push({
					marketIndex: spotMarketAccount.marketIndex,
					size: netQuoteValue,
					value: baseAssetValue,
					weight: weight,
					weightedValue: weightedTokenValue,
				});
			}
		}

		return healthComponents;
	}

	/**
	 * Get the total position value, excluding any position coming from the given target market
	 * @param marketToIgnore
	 * @returns positionValue : Precision QUOTE_PRECISION
	 */
	private getTotalVaultPositionValueExcludingMarket(
		marketToIgnore: number,
		marginCategory?: MarginCategory,
		liquidationBuffer?: BN,
		includeOpenOrders?: boolean
	): BN {
		const currentVaultPosition =
			this.getVaultPositionWithLPSettle(
				marketToIgnore,
				undefined,
				!!marginCategory
			)[0] || this.getEmptyPosition(marketToIgnore);

		const oracleData = this.getOracleDataForMarket(marketToIgnore);

		let currentVaultPositionValueUSDC = ZERO;
		if (currentVaultPosition) {
			currentVaultPositionValueUSDC = this.getPerpLiabilityValue(
				marketToIgnore,
				oracleData,
				includeOpenOrders
			);
		}

		return this.getTotalVaultPositionLiability(
			marginCategory,
			liquidationBuffer,
			includeOpenOrders
		).sub(currentVaultPositionValueUSDC);
	}

	private getOracleDataForMarket(marketIndex: number): OraclePriceData {
		return this.normalClient.getOracleDataForMarket(marketIndex);
	}

	private getOracleDataForVault(vaultIndex: number): OraclePriceData {
		return this.normalClient.getOracleDataForVault(vaultIndex);
	}
}
