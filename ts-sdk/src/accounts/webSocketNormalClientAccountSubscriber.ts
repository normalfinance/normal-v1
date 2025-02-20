import {
	AccountSubscriber,
	DataAndSlot,
	DelistedMarketSetting,
	NormalClientAccountEvents,
	NormalClientAccountSubscriber,
	NotSubscribedError,
	ResubOpts,
} from './types';
import { MarketAccount, StateAccount, InsuranceFundAccount } from '../types';
import { Program } from '@coral-xyz/anchor';
import StrictEventEmitter from 'strict-event-emitter-types';
import { EventEmitter } from 'events';
import {
	getNormalStateAccountPublicKey,
	getMarketPublicKey,
	getMarketPublicKeySync,
} from '../addresses/pda';
import { WebSocketAccountSubscriber } from './webSocketAccountSubscriber';
import { Commitment, PublicKey } from '@solana/web3.js';
import { OracleInfo, OraclePriceData } from '../oracles/types';
import { OracleClientCache } from '../oracles/oracleClientCache';
import * as Buffer from 'buffer';
import { QUOTE_ORACLE_PRICE_DATA } from '../oracles/quoteAssetOracleClient';
import { findAllMarketAndOracles } from '../config';
import { findDelistedMarketsAndOracles } from './utils';

const ORACLE_DEFAULT_KEY = PublicKey.default.toBase58();

export class WebSocketNormalClientAccountSubscriber
	implements NormalClientAccountSubscriber
{
	isSubscribed: boolean;
	program: Program;
	commitment?: Commitment;
	marketIndexes: number[];
	oracleInfos: OracleInfo[];
	oracleClientCache = new OracleClientCache();

	resubOpts?: ResubOpts;
	shouldFindAllMarketsAndOracles: boolean;

	eventEmitter: StrictEventEmitter<EventEmitter, NormalClientAccountEvents>;
	stateAccountSubscriber?: AccountSubscriber<StateAccount>;
	marketAccountSubscribers = new Map<
		number,
		AccountSubscriber<MarketAccount>
	>();
	oracleMap = new Map<number, PublicKey>();
	oracleStringMap = new Map<number, string>();

	oracleSubscribers = new Map<string, AccountSubscriber<OraclePriceData>>();
	delistedMarketSetting: DelistedMarketSetting;

	initialMarketAccountData: Map<number, MarketAccount>;
	initialOraclePriceData: Map<string, OraclePriceData>;

	private isSubscribing = false;
	private subscriptionPromise: Promise<boolean>;
	private subscriptionPromiseResolver: (val: boolean) => void;

	public constructor(
		program: Program,
		marketIndexes: number[],
		oracleInfos: OracleInfo[],
		shouldFindAllMarketsAndOracles: boolean,
		delistedMarketSetting: DelistedMarketSetting,
		resubOpts?: ResubOpts,
		commitment?: Commitment
	) {
		this.isSubscribed = false;
		this.program = program;
		this.eventEmitter = new EventEmitter();
		this.marketIndexes = marketIndexes;
		this.oracleInfos = oracleInfos;
		this.shouldFindAllMarketsAndOracles = shouldFindAllMarketsAndOracles;
		this.delistedMarketSetting = delistedMarketSetting;
		this.resubOpts = resubOpts;
		this.commitment = commitment;
	}
	getOraclePriceDataAndSlotForVault(
		vaultIndex: number
	): DataAndSlot<OraclePriceData> | undefined {
		throw new Error('Method not implemented.');
	}
	getInsuranceAccountAndSlot(): DataAndSlot<InsuranceFundAccount> {
		throw new Error('Method not implemented.');
	}
	getOraclePriceDataAndSlotForPerpMarket(
		marketIndex: number
	): DataAndSlot<OraclePriceData> | undefined {
		throw new Error('Method not implemented.');
	}
	getOraclePriceDataAndSlotForSpotMarket(
		marketIndex: number
	): DataAndSlot<OraclePriceData> | undefined {
		throw new Error('Method not implemented.');
	}
	updateAccountLoaderPollingFrequency?: (pollingFrequency: number) => void;

	public async subscribe(): Promise<boolean> {
		if (this.isSubscribed) {
			return true;
		}

		if (this.isSubscribing) {
			return await this.subscriptionPromise;
		}

		this.isSubscribing = true;

		this.subscriptionPromise = new Promise((res) => {
			this.subscriptionPromiseResolver = res;
		});

		if (this.shouldFindAllMarketsAndOracles) {
			const { marketIndexes, marketAccounts, oracleInfos } =
				await findAllMarketAndOracles(this.program);
			this.marketIndexes = marketIndexes;
			this.oracleInfos = oracleInfos;
			// front run and set the initial data here to save extra gma call in set initial data
			this.initialMarketAccountData = new Map(
				marketAccounts.map((market) => [market.marketIndex, market])
			);
		}

		const statePublicKey = await getNormalStateAccountPublicKey(
			this.program.programId
		);

		// create and activate main state account subscription
		this.stateAccountSubscriber = new WebSocketAccountSubscriber(
			'state',
			this.program,
			statePublicKey,
			undefined,
			undefined,
			this.commitment
		);
		await this.stateAccountSubscriber.subscribe((data: StateAccount) => {
			this.eventEmitter.emit('stateAccountUpdate', data);
			this.eventEmitter.emit('update');
		});

		// set initial data to avoid spamming getAccountInfo calls in webSocketAccountSubscriber
		await this.setInitialData();

		await Promise.all([
			// subscribe to market accounts
			this.subscribeToMarketAccounts(),
			// subscribe to oracles
			this.subscribeToOracles(),
		]);

		this.eventEmitter.emit('update');

		await this.handleDelistedMarkets();

		await Promise.all([this.setOracleMap()]);

		this.isSubscribing = false;
		this.isSubscribed = true;
		this.subscriptionPromiseResolver(true);

		// delete initial data
		this.removeInitialData();

		return true;
	}

	async setInitialData(): Promise<void> {
		const connection = this.program.provider.connection;

		if (!this.initialMarketAccountData) {
			const marketPublicKeys = this.marketIndexes.map((marketIndex) =>
				getMarketPublicKeySync(this.program.programId, marketIndex)
			);
			const marketAccountInfos = await connection.getMultipleAccountsInfo(
				marketPublicKeys
			);
			this.initialMarketAccountData = new Map(
				marketAccountInfos
					.filter((accountInfo) => !!accountInfo)
					.map((accountInfo) => {
						const market = this.program.coder.accounts.decode(
							'Market',
							accountInfo.data
						);
						return [market.marketIndex, market];
					})
			);
		}

		const oracleAccountInfos = await connection.getMultipleAccountsInfo(
			this.oracleInfos.map((oracleInfo) => oracleInfo.publicKey)
		);
		this.initialOraclePriceData = new Map(
			this.oracleInfos.reduce((result, oracleInfo, i) => {
				if (!oracleAccountInfos[i]) {
					return result;
				}

				const oracleClient = this.oracleClientCache.get(
					oracleInfo.source,
					connection
				);

				const oraclePriceData = oracleClient.getOraclePriceDataFromBuffer(
					oracleAccountInfos[i].data
				);

				result.push([oracleInfo.publicKey.toString(), oraclePriceData]);
				return result;
			}, [])
		);
	}

	removeInitialData() {
		this.initialMarketAccountData = new Map();
		this.initialOraclePriceData = new Map();
	}

	async subscribeToMarketAccounts(): Promise<boolean> {
		await Promise.all(
			this.marketIndexes.map((marketIndex) =>
				this.subscribeToMarketAccount(marketIndex)
			)
		);
		return true;
	}

	async subscribeToMarketAccount(marketIndex: number): Promise<boolean> {
		const marketPublicKey = await getMarketPublicKey(
			this.program.programId,
			marketIndex
		);
		const accountSubscriber = new WebSocketAccountSubscriber<MarketAccount>(
			'market',
			this.program,
			marketPublicKey,
			undefined,
			this.resubOpts,
			this.commitment
		);
		accountSubscriber.setData(this.initialMarketAccountData.get(marketIndex));
		await accountSubscriber.subscribe((data: MarketAccount) => {
			this.eventEmitter.emit('marketAccountUpdate', data);
			this.eventEmitter.emit('update');
		});
		this.marketAccountSubscribers.set(marketIndex, accountSubscriber);
		return true;
	}

	async subscribeToOracles(): Promise<boolean> {
		await Promise.all(
			this.oracleInfos
				.filter((oracleInfo) => !oracleInfo.publicKey.equals(PublicKey.default))
				.map((oracleInfo) => this.subscribeToOracle(oracleInfo))
		);

		return true;
	}

	async subscribeToOracle(oracleInfo: OracleInfo): Promise<boolean> {
		const oracleString = oracleInfo.publicKey.toString();
		const client = this.oracleClientCache.get(
			oracleInfo.source,
			this.program.provider.connection
		);
		const accountSubscriber = new WebSocketAccountSubscriber<OraclePriceData>(
			'oracle',
			this.program,
			oracleInfo.publicKey,
			(buffer: Buffer) => {
				return client.getOraclePriceDataFromBuffer(buffer);
			},
			this.resubOpts,
			this.commitment
		);
		const initialOraclePriceData =
			this.initialOraclePriceData.get(oracleString);
		if (initialOraclePriceData) {
			accountSubscriber.setData(initialOraclePriceData);
		}
		await accountSubscriber.subscribe((data: OraclePriceData) => {
			this.eventEmitter.emit('oraclePriceUpdate', oracleInfo.publicKey, data);
			this.eventEmitter.emit('update');
		});

		this.oracleSubscribers.set(oracleString, accountSubscriber);
		return true;
	}

	async unsubscribeFromMarketAccounts(): Promise<void> {
		await Promise.all(
			Array.from(this.marketAccountSubscribers.values()).map(
				(accountSubscriber) => accountSubscriber.unsubscribe()
			)
		);
	}

	async unsubscribeFromOracles(): Promise<void> {
		await Promise.all(
			Array.from(this.oracleSubscribers.values()).map((accountSubscriber) =>
				accountSubscriber.unsubscribe()
			)
		);
	}

	public async fetch(): Promise<void> {
		if (!this.isSubscribed) {
			return;
		}

		const promises = [this.stateAccountSubscriber.fetch()].concat(
			Array.from(this.marketAccountSubscribers.values()).map((subscriber) =>
				subscriber.fetch()
			)
		);

		await Promise.all(promises);
	}

	public async unsubscribe(): Promise<void> {
		if (!this.isSubscribed) {
			return;
		}

		await this.stateAccountSubscriber.unsubscribe();

		await this.unsubscribeFromMarketAccounts();
		await this.unsubscribeFromOracles();

		this.isSubscribed = false;
	}

	async addMarket(marketIndex: number): Promise<boolean> {
		if (this.marketAccountSubscribers.has(marketIndex)) {
			return true;
		}
		const subscriptionSuccess = this.subscribeToMarketAccount(marketIndex);
		await this.setOracleMap();
		return subscriptionSuccess;
	}

	async addOracle(oracleInfo: OracleInfo): Promise<boolean> {
		if (this.oracleSubscribers.has(oracleInfo.publicKey.toString())) {
			return true;
		}

		if (oracleInfo.publicKey.equals(PublicKey.default)) {
			return true;
		}

		return this.subscribeToOracle(oracleInfo);
	}

	async setOracleMap() {
		const markets = this.getMarketAccountsAndSlots();
		const addOraclePromises = [];
		for (const market of markets) {
			if (!market || !market.data) {
				continue;
			}
			const marketAccount = market.data;
			const marketIndex = marketAccount.marketIndex;
			const oracle = marketAccount.amm.oracle;
			if (!this.oracleSubscribers.has(oracle.toBase58())) {
				addOraclePromises.push(
					this.addOracle({
						publicKey: oracle,
						source: market.data.amm.oracleSource,
					})
				);
			}
			this.oracleMap.set(marketIndex, oracle);
			this.oracleStringMap.set(marketIndex, oracle.toBase58());
		}
		await Promise.all(addOraclePromises);
	}

	async handleDelistedMarkets(): Promise<void> {
		if (this.delistedMarketSetting === DelistedMarketSetting.Subscribe) {
			return;
		}

		const { marketIndexes, oracles } = findDelistedMarketsAndOracles(
			this.getMarketAccountsAndSlots()
		);

		for (const marketIndex of marketIndexes) {
			await this.marketAccountSubscribers.get(marketIndex).unsubscribe();
			if (this.delistedMarketSetting === DelistedMarketSetting.Discard) {
				this.marketAccountSubscribers.delete(marketIndex);
			}
		}

		for (const oracle of oracles) {
			await this.oracleSubscribers.get(oracle.toBase58()).unsubscribe();
			if (this.delistedMarketSetting === DelistedMarketSetting.Discard) {
				this.oracleSubscribers.delete(oracle.toBase58());
			}
		}
	}

	assertIsSubscribed(): void {
		if (!this.isSubscribed) {
			throw new NotSubscribedError(
				'You must call `subscribe` before using this function'
			);
		}
	}

	public getStateAccountAndSlot(): DataAndSlot<StateAccount> {
		this.assertIsSubscribed();
		return this.stateAccountSubscriber.dataAndSlot;
	}

	public getMarketAccountAndSlot(
		marketIndex: number
	): DataAndSlot<MarketAccount> | undefined {
		this.assertIsSubscribed();
		return this.marketAccountSubscribers.get(marketIndex).dataAndSlot;
	}

	public getMarketAccountsAndSlots(): DataAndSlot<MarketAccount>[] {
		return Array.from(this.marketAccountSubscribers.values()).map(
			(subscriber) => subscriber.dataAndSlot
		);
	}

	public getOraclePriceDataAndSlot(
		oraclePublicKey: PublicKey | string
	): DataAndSlot<OraclePriceData> | undefined {
		this.assertIsSubscribed();
		const oracleString =
			typeof oraclePublicKey === 'string'
				? oraclePublicKey
				: oraclePublicKey.toBase58();
		if (oracleString === ORACLE_DEFAULT_KEY) {
			return {
				data: QUOTE_ORACLE_PRICE_DATA,
				slot: 0,
			};
		}
		return this.oracleSubscribers.get(oracleString).dataAndSlot;
	}

	public getOraclePriceDataAndSlotForMarket(
		marketIndex: number
	): DataAndSlot<OraclePriceData> | undefined {
		const marketAccount = this.getMarketAccountAndSlot(marketIndex);
		const oracle = this.oracleMap.get(marketIndex);
		const oracleString = this.oracleStringMap.get(marketIndex);
		if (!marketAccount || !oracle) {
			return undefined;
		}

		if (!marketAccount.data.amm.oracle.equals(oracle)) {
			// If the oracle has changed, we need to update the oracle map in background
			this.setOracleMap();
		}

		return this.getOraclePriceDataAndSlot(oracleString);
	}
}
