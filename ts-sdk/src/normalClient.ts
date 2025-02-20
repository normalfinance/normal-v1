import * as anchor from '@coral-xyz/anchor';
import {
	AnchorProvider,
	BN,
	Idl,
	Program,
	ProgramAccount,
} from '@coral-xyz/anchor';
import { Idl as Idl30, Program as Program30 } from '@coral-xyz/anchor-30';
import bs58 from 'bs58';
import {
	ASSOCIATED_TOKEN_PROGRAM_ID,
	createAssociatedTokenAccountInstruction,
	createCloseAccountInstruction,
	createInitializeAccountInstruction,
	getAssociatedTokenAddress,
	TOKEN_2022_PROGRAM_ID,
	TOKEN_PROGRAM_ID,
} from '@solana/spl-token';
import {
	NormalClientMetricsEvents,
	IWallet,
	ReferrerInfo,
	ReferrerNameAccount,
	SignedTxData,
	StateAccount,
	TxParams,
	UserAccount,
	UserStatsAccount,
	InsuranceFundAccount,
	MarketAccount,
	isVariant,
} from './types';
import normalIDL from './idl/normal.json';

import {
	AccountMeta,
	AddressLookupTableAccount,
	BlockhashWithExpiryBlockHeight,
	ConfirmOptions,
	Connection,
	Ed25519Program,
	Keypair,
	LAMPORTS_PER_SOL,
	PublicKey,
	Signer,
	SystemProgram,
	SYSVAR_INSTRUCTIONS_PUBKEY,
	Transaction,
	TransactionInstruction,
	TransactionSignature,
	TransactionVersion,
	VersionedTransaction,
} from '@solana/web3.js';

import { TokenFaucet } from './tokenFaucet';
import { EventEmitter } from 'events';
import StrictEventEmitter from 'strict-event-emitter-types';
import {
	getNormalSignerPublicKey,
	getNormalStateAccountPublicKey,
	getInsuranceFundStakeAccountPublicKey,
	getPythPullOraclePublicKey,
	getReferrerNamePublicKeySync,
	getUserAccountPublicKey,
	getUserAccountPublicKeySync,
	getUserStatsAccountPublicKey,
} from './addresses/pda';
import {
	DataAndSlot,
	DelistedMarketSetting,
	NormalClientAccountEvents,
	NormalClientAccountSubscriber,
} from './accounts/types';
import { TxSender, TxSigAndSlot } from './tx/types';
import {
	BASE_PRECISION,
	PRICE_PRECISION,
	QUOTE_PRECISION,
	QUOTE_SPOT_MARKET_INDEX,
	ZERO,
} from './constants/numericConstants';
import { decodeName, DEFAULT_USER_NAME, encodeName } from './userName';
import { OraclePriceData } from './oracles/types';
import { NormalClientConfig } from './normalClientConfig';
import { PollingNormalClientAccountSubscriber } from './accounts/pollingNormalClientAccountSubscriber';
import { WebSocketNormalClientAccountSubscriber } from './accounts/webSocketNormalClientAccountSubscriber';
import { RetryTxSender } from './tx/retryTxSender';
import { User } from './user';
import { UserSubscriptionConfig } from './userConfig';
import {
	configs,
	NORMAL_ORACLE_RECEIVER_ID,
	DEFAULT_CONFIRMATION_OPTS,
	NORMAL_PROGRAM_ID,
} from './config';
import { WRAPPED_SOL_MINT } from './constants/markets';
import { UserStats } from './userStats';
import { fetchUserStatsAccount } from './accounts/fetch';

import { getNonIdleUserFilter } from './memcmp';
import { UserStatsSubscriptionConfig } from './userStatsConfig';
import { numberToSafeBN } from './math/utils';
import { TransactionParamProcessor } from './tx/txParamProcessor';
import { trimVaaSignatures } from './math/oracles';
import { TxHandler } from './tx/txHandler';
import {
	DEFAULT_RECEIVER_PROGRAM_ID,
	wormholeCoreBridgeIdl,
} from '@pythnetwork/pyth-solana-receiver';
import { parseAccumulatorUpdateData } from '@pythnetwork/price-service-sdk';
import {
	DEFAULT_WORMHOLE_PROGRAM_ID,
	getGuardianSetPda,
} from '@pythnetwork/pyth-solana-receiver/lib/address';
import { WormholeCoreBridgeSolana } from '@pythnetwork/pyth-solana-receiver/lib/idl/wormhole_core_bridge_solana';
import { PythSolanaReceiver } from '@pythnetwork/pyth-solana-receiver/lib/idl/pyth_solana_receiver';
import { getFeedIdUint8Array, trimFeedId } from './util/pythPullOracleUtils';
import { isVersionedTransaction } from './tx/utils';
import pythSolanaReceiverIdl from './idl/pyth_solana_receiver.json';
import * as ed from '@noble/ed25519';

type RemainingAccountParams = {
	userAccounts: UserAccount[];
	writableMarketIndexes?: number[];
	readableMarketIndex?: number | number[];
	useMarketLastSlotCache?: boolean;
};

/**
 * # NormalClient
 * This class is the main way to interact with Normal Protocol. It allows you to subscribe to the various accounts where the Market's state is stored, as well as: opening positions, liquidating, settling funding, depositing & withdrawing, and more.
 */
export class NormalClient {
	connection: Connection;
	wallet: IWallet;
	public program: Program;
	provider: AnchorProvider;
	opts?: ConfirmOptions;
	users = new Map<string, User>();
	userStats?: UserStats;
	activeSubAccountId: number;
	userAccountSubscriptionConfig: UserSubscriptionConfig;
	userStatsAccountSubscriptionConfig: UserStatsSubscriptionConfig;
	accountSubscriber: NormalClientAccountSubscriber;
	eventEmitter: StrictEventEmitter<EventEmitter, NormalClientAccountEvents>;
	metricsEventEmitter: StrictEventEmitter<
		EventEmitter,
		NormalClientMetricsEvents
	>;
	_isSubscribed = false;
	txSender: TxSender;
	marketLastSlotCache = new Map<number, number>();
	mustIncludeMarketIndexes = new Set<number>();
	authority: PublicKey;
	marketLookupTable: PublicKey;
	lookupTableAccount: AddressLookupTableAccount;
	includeDelegates?: boolean;
	authoritySubAccountMap?: Map<string, number[]>;
	skipLoadUsers?: boolean;
	txVersion: TransactionVersion;
	txParams: TxParams;
	enableMetricsEvents?: boolean;
	txHandler: TxHandler;
	receiverProgram?: Program<PythSolanaReceiver>;
	wormholeProgram?: Program<WormholeCoreBridgeSolana>;

	public get isSubscribed() {
		return this._isSubscribed && this.accountSubscriber.isSubscribed;
	}

	public set isSubscribed(val: boolean) {
		this._isSubscribed = val;
	}

	public constructor(config: NormalClientConfig) {
		this.connection = config.connection;
		this.wallet = config.wallet;
		this.opts = config.opts || {
			...DEFAULT_CONFIRMATION_OPTS,
			commitment: config?.connection?.commitment,
			preflightCommitment: config?.connection?.commitment, // At the moment this ensures that our transaction simulations (which use Connection object) will use the same commitment level as our Transaction blockhashes (which use these opts)
		};
		this.provider = new AnchorProvider(
			config.connection,
			// @ts-ignore
			config.wallet,
			this.opts
		);
		this.program = new Program(
			normalIDL as Idl,
			config.programID ?? new PublicKey(NORMAL_PROGRAM_ID),
			this.provider
		);

		this.authority = config.authority ?? this.wallet.publicKey;
		this.activeSubAccountId = config.activeSubAccountId ?? 0;
		this.skipLoadUsers = config.skipLoadUsers ?? false;
		this.txVersion = config.txVersion ?? 'legacy';
		this.txParams = {
			computeUnits: config.txParams?.computeUnits ?? 600_000,
			computeUnitsPrice: config.txParams?.computeUnitsPrice ?? 0,
		};

		this.txHandler =
			config?.txHandler ??
			new TxHandler({
				connection: this.connection,
				// @ts-ignore
				wallet: this.provider.wallet,
				confirmationOptions: this.opts,
				opts: {
					returnBlockHeightsWithSignedTxCallbackData:
						config.enableMetricsEvents,
					onSignedCb: this.handleSignedTransaction.bind(this),
					preSignedCb: this.handlePreSignedTransaction.bind(this),
				},
				config: config.txHandlerConfig,
			});

		if (config.includeDelegates && config.subAccountIds) {
			throw new Error(
				'Can only pass one of includeDelegates or subAccountIds. If you want to specify subaccount ids for multiple authorities, pass authoritySubaccountMap instead'
			);
		}

		if (config.authoritySubAccountMap && config.subAccountIds) {
			throw new Error(
				'Can only pass one of authoritySubaccountMap or subAccountIds'
			);
		}

		if (config.authoritySubAccountMap && config.includeDelegates) {
			throw new Error(
				'Can only pass one of authoritySubaccountMap or includeDelegates'
			);
		}

		this.authoritySubAccountMap = config.authoritySubAccountMap
			? config.authoritySubAccountMap
			: config.subAccountIds
			? new Map([[this.authority.toString(), config.subAccountIds]])
			: new Map<string, number[]>();

		this.includeDelegates = config.includeDelegates ?? false;
		if (config.accountSubscription?.type === 'polling') {
			this.userAccountSubscriptionConfig = {
				type: 'polling',
				accountLoader: config.accountSubscription.accountLoader,
			};
			this.userStatsAccountSubscriptionConfig = {
				type: 'polling',
				accountLoader: config.accountSubscription.accountLoader,
			};
		} else {
			this.userAccountSubscriptionConfig = {
				type: 'websocket',
				resubTimeoutMs: config.accountSubscription?.resubTimeoutMs,
				logResubMessages: config.accountSubscription?.logResubMessages,
				commitment: config.accountSubscription?.commitment,
			};
			this.userStatsAccountSubscriptionConfig = {
				type: 'websocket',
				resubTimeoutMs: config.accountSubscription?.resubTimeoutMs,
				logResubMessages: config.accountSubscription?.logResubMessages,
				commitment: config.accountSubscription?.commitment,
			};
		}

		if (config.userStats) {
			this.userStats = new UserStats({
				normalClient: this,
				userStatsAccountPublicKey: getUserStatsAccountPublicKey(
					this.program.programId,
					this.authority
				),
				accountSubscription: this.userAccountSubscriptionConfig,
			});
		}

		this.marketLookupTable = config.marketLookupTable;
		if (config.env && !this.marketLookupTable) {
			this.marketLookupTable = new PublicKey(
				configs[config.env].MARKET_LOOKUP_TABLE
			);
		}

		const delistedMarketSetting =
			config.delistedMarketSetting || DelistedMarketSetting.Subscribe;
		const noMarketsAndOraclesSpecified =
			// config.marketIndexes === undefined &&
			config.oracleInfos === undefined;
		if (config.accountSubscription?.type === 'polling') {
			this.accountSubscriber = new PollingNormalClientAccountSubscriber(
				this.program,
				config.accountSubscription.accountLoader,
				config.marketIndexes ?? [],
				config.oracleInfos ?? [],
				noMarketsAndOraclesSpecified,
				delistedMarketSetting
			);
		} else {
			this.accountSubscriber = new WebSocketNormalClientAccountSubscriber(
				this.program,
				config.marketIndexes ?? [],
				config.oracleInfos ?? [],
				noMarketsAndOraclesSpecified,
				delistedMarketSetting,
				{
					resubTimeoutMs: config.accountSubscription?.resubTimeoutMs,
					logResubMessages: config.accountSubscription?.logResubMessages,
				},
				config.accountSubscription?.commitment
			);
		}
		this.eventEmitter = this.accountSubscriber.eventEmitter;

		this.metricsEventEmitter = new EventEmitter();

		if (config.enableMetricsEvents) {
			this.enableMetricsEvents = true;
		}

		this.txSender =
			config.txSender ??
			new RetryTxSender({
				connection: this.connection,
				wallet: this.wallet,
				opts: this.opts,
				txHandler: this.txHandler,
			});
	}

	public getUserMapKey(subAccountId: number, authority: PublicKey): string {
		return `${subAccountId}_${authority.toString()}`;
	}

	createUser(
		subAccountId: number,
		accountSubscriptionConfig: UserSubscriptionConfig,
		authority?: PublicKey
	): User {
		const userAccountPublicKey = getUserAccountPublicKeySync(
			this.program.programId,
			authority ?? this.authority,
			subAccountId
		);

		return new User({
			normalClient: this,
			userAccountPublicKey,
			accountSubscription: accountSubscriptionConfig,
		});
	}

	public async subscribe(): Promise<boolean> {
		let subscribePromises = [this.addAndSubscribeToUsers()].concat(
			this.accountSubscriber.subscribe()
		);

		if (this.userStats !== undefined) {
			subscribePromises = subscribePromises.concat(this.userStats.subscribe());
		}
		this.isSubscribed = (await Promise.all(subscribePromises)).reduce(
			(success, prevSuccess) => success && prevSuccess
		);

		return this.isSubscribed;
	}

	subscribeUsers(): Promise<boolean>[] {
		return [...this.users.values()].map((user) => user.subscribe());
	}

	/**
	 *	Forces the accountSubscriber to fetch account updates from rpc
	 */
	public async fetchAccounts(): Promise<void> {
		let promises = [...this.users.values()]
			.map((user) => user.fetchAccounts())
			.concat(this.accountSubscriber.fetch());
		if (this.userStats) {
			promises = promises.concat(this.userStats.fetchAccounts());
		}
		await Promise.all(promises);
	}

	public async unsubscribe(): Promise<void> {
		let unsubscribePromises = this.unsubscribeUsers().concat(
			this.accountSubscriber.unsubscribe()
		);
		if (this.userStats !== undefined) {
			unsubscribePromises = unsubscribePromises.concat(
				this.userStats.unsubscribe()
			);
		}
		await Promise.all(unsubscribePromises);
		this.isSubscribed = false;
	}

	unsubscribeUsers(): Promise<void>[] {
		return [...this.users.values()].map((user) => user.unsubscribe());
	}

	statePublicKey?: PublicKey;
	public async getStatePublicKey(): Promise<PublicKey> {
		if (this.statePublicKey) {
			return this.statePublicKey;
		}
		this.statePublicKey = await getNormalStateAccountPublicKey(
			this.program.programId
		);
		return this.statePublicKey;
	}

	signerPublicKey?: PublicKey;
	public getSignerPublicKey(): PublicKey {
		if (this.signerPublicKey) {
			return this.signerPublicKey;
		}
		this.signerPublicKey = getNormalSignerPublicKey(this.program.programId);
		return this.signerPublicKey;
	}

	public getStateAccount(): StateAccount {
		return this.accountSubscriber.getStateAccountAndSlot().data;
	}

	/**
	 * Forces a fetch to rpc before returning accounts. Useful for anchor tests.
	 */
	public async forceGetStateAccount(): Promise<StateAccount> {
		await this.accountSubscriber.fetch();
		return this.accountSubscriber.getStateAccountAndSlot().data;
	}

	public getInsuranceFundAccount(): InsuranceFundAccount {
		return this.accountSubscriber.getInsuranceAccountAndSlot().data;
	}

	/**
	 * Forces a fetch to rpc before returning accounts. Useful for anchor tests.
	 */
	public async forceGetInsuranceFundAccount(): Promise<InsuranceFundAccount> {
		await this.accountSubscriber.fetch();
		return this.accountSubscriber.getInsuranceAccountAndSlot().data;
	}

	public getMarketAccount(marketIndex: number): MarketAccount | undefined {
		return this.accountSubscriber.getMarketAccountAndSlot(marketIndex)?.data;
	}

	/**
	 * Forces a fetch to rpc before returning accounts. Useful for anchor tests.
	 * @param marketIndex
	 */
	public async forceGetMarketAccount(
		marketIndex: number
	): Promise<MarketAccount | undefined> {
		await this.accountSubscriber.fetch();
		let data =
			this.accountSubscriber.getMarketAccountAndSlot(marketIndex)?.data;
		let i = 0;
		while (data === undefined && i < 10) {
			await this.accountSubscriber.fetch();
			data = this.accountSubscriber.getMarketAccountAndSlot(marketIndex)?.data;
			i++;
		}
		return data;
	}

	public getMarketAccounts(): MarketAccount[] {
		return this.accountSubscriber
			.getMarketAccountsAndSlots()
			.filter((value) => value !== undefined)
			.map((value) => value.data);
	}

	public getOraclePriceDataAndSlot(
		oraclePublicKey: PublicKey
	): DataAndSlot<OraclePriceData> | undefined {
		return this.accountSubscriber.getOraclePriceDataAndSlot(
			oraclePublicKey.toBase58()
		);
	}

	public async fetchMarketLookupTableAccount(): Promise<AddressLookupTableAccount> {
		if (this.lookupTableAccount) return this.lookupTableAccount;

		if (!this.marketLookupTable) {
			console.log('Market lookup table address not set');
			return;
		}

		const lookupTableAccount = (
			await this.connection.getAddressLookupTable(this.marketLookupTable)
		).value;
		this.lookupTableAccount = lookupTableAccount;

		return lookupTableAccount;
	}

	/**
	 * Update the wallet to use for normal transactions and linked user account
	 * @param newWallet
	 * @param subAccountIds
	 * @param activeSubAccountId
	 * @param includeDelegates
	 */
	public async updateWallet(
		newWallet: IWallet,
		subAccountIds?: number[],
		activeSubAccountId?: number,
		includeDelegates?: boolean,
		authoritySubaccountMap?: Map<string, number[]>
	): Promise<boolean> {
		const newProvider = new AnchorProvider(
			this.connection,
			// @ts-ignore
			newWallet,
			this.opts
		);
		const newProgram = new Program(
			normalIDL as Idl,
			this.program.programId,
			newProvider
		);

		this.skipLoadUsers = false;
		// Update provider for txSender with new wallet details
		this.txSender.wallet = newWallet;
		this.wallet = newWallet;
		this.txHandler.updateWallet(newWallet);
		this.provider = newProvider;
		this.program = newProgram;
		this.authority = newWallet.publicKey;
		this.activeSubAccountId = activeSubAccountId;
		this.userStatsAccountPublicKey = undefined;
		this.includeDelegates = includeDelegates ?? false;
		const walletSupportsVersionedTxns =
			//@ts-ignore
			this.wallet.supportedTransactionVersions?.size ?? 0 > 1;
		this.txVersion = walletSupportsVersionedTxns ? 0 : 'legacy';

		if (includeDelegates && subAccountIds) {
			throw new Error(
				'Can only pass one of includeDelegates or subAccountIds. If you want to specify subaccount ids for multiple authorities, pass authoritySubaccountMap instead'
			);
		}

		if (authoritySubaccountMap && subAccountIds) {
			throw new Error(
				'Can only pass one of authoritySubaccountMap or subAccountIds'
			);
		}

		if (authoritySubaccountMap && includeDelegates) {
			throw new Error(
				'Can only pass one of authoritySubaccountMap or includeDelegates'
			);
		}

		this.authoritySubAccountMap = authoritySubaccountMap
			? authoritySubaccountMap
			: subAccountIds
			? new Map([[this.authority.toString(), subAccountIds]])
			: new Map<string, number[]>();

		/* Reset user stats account */
		if (this.userStats?.isSubscribed) {
			await this.userStats.unsubscribe();
		}

		this.userStats = undefined;

		this.userStats = new UserStats({
			normalClient: this,
			userStatsAccountPublicKey: this.getUserStatsAccountPublicKey(),
			accountSubscription: this.userStatsAccountSubscriptionConfig,
		});

		await this.userStats.subscribe();

		let success = true;

		if (this.isSubscribed) {
			await Promise.all(this.unsubscribeUsers());
			this.users.clear();
			success = await this.addAndSubscribeToUsers();
		}

		return success;
	}

	/**
	 * Update the subscribed accounts to a given authority, while leaving the
	 * connected wallet intact. This allows a user to emulate another user's
	 * account on the UI and sign permissionless transactions with their own wallet.
	 * @param emulateAuthority
	 */
	public async emulateAccount(emulateAuthority: PublicKey): Promise<boolean> {
		this.skipLoadUsers = false;
		// Update provider for txSender with new wallet details
		this.authority = emulateAuthority;
		this.userStatsAccountPublicKey = undefined;
		this.includeDelegates = true;
		const walletSupportsVersionedTxns =
			//@ts-ignore
			this.wallet.supportedTransactionVersions?.size ?? 0 > 1;
		this.txVersion = walletSupportsVersionedTxns ? 0 : 'legacy';

		this.authoritySubAccountMap = new Map<string, number[]>();

		/* Reset user stats account */
		if (this.userStats?.isSubscribed) {
			await this.userStats.unsubscribe();
		}

		this.userStats = undefined;

		this.userStats = new UserStats({
			normalClient: this,
			userStatsAccountPublicKey: this.getUserStatsAccountPublicKey(),
			accountSubscription: this.userStatsAccountSubscriptionConfig,
		});

		await this.userStats.subscribe();

		let success = true;

		if (this.isSubscribed) {
			await Promise.all(this.unsubscribeUsers());
			this.users.clear();
			success = await this.addAndSubscribeToUsers(emulateAuthority);
		}

		return success;
	}

	public async switchActiveUser(subAccountId: number, authority?: PublicKey) {
		const authorityChanged = authority && !this.authority?.equals(authority);

		this.activeSubAccountId = subAccountId;
		this.authority = authority ?? this.authority;
		this.userStatsAccountPublicKey = getUserStatsAccountPublicKey(
			this.program.programId,
			this.authority
		);

		/* If changing the user authority ie switching from delegate to non-delegate account, need to re-subscribe to the user stats account */
		if (authorityChanged) {
			if (this.userStats && this.userStats.isSubscribed) {
				await this.userStats.unsubscribe();
			}

			this.userStats = new UserStats({
				normalClient: this,
				userStatsAccountPublicKey: this.userStatsAccountPublicKey,
				accountSubscription: this.userAccountSubscriptionConfig,
			});

			this.userStats.subscribe();
		}
	}

	public async addUser(
		subAccountId: number,
		authority?: PublicKey,
		userAccount?: UserAccount
	): Promise<boolean> {
		authority = authority ?? this.authority;
		const userKey = this.getUserMapKey(subAccountId, authority);

		if (this.users.has(userKey) && this.users.get(userKey).isSubscribed) {
			return true;
		}

		const user = this.createUser(
			subAccountId,
			this.userAccountSubscriptionConfig,
			authority
		);

		const result = await user.subscribe(userAccount);

		if (result) {
			this.users.set(userKey, user);
			return true;
		} else {
			return false;
		}
	}

	/**
	 * Adds and subscribes to users based on params set by the constructor or by updateWallet.
	 */
	public async addAndSubscribeToUsers(authority?: PublicKey): Promise<boolean> {
		// save the rpc calls if normalclient is initialized without a real wallet
		if (this.skipLoadUsers) return true;

		let result = true;

		if (this.authoritySubAccountMap && this.authoritySubAccountMap.size > 0) {
			this.authoritySubAccountMap.forEach(async (value, key) => {
				for (const subAccountId of value) {
					result =
						result && (await this.addUser(subAccountId, new PublicKey(key)));
				}
			});

			if (this.activeSubAccountId == undefined) {
				this.switchActiveUser(
					[...this.authoritySubAccountMap.values()][0][0] ?? 0,
					new PublicKey(
						[...this.authoritySubAccountMap.keys()][0] ??
							this.authority.toString()
					)
				);
			}
		} else {
			let userAccounts = [];
			let delegatedAccounts = [];

			const userAccountsPromise = this.getUserAccountsForAuthority(
				authority ?? this.wallet.publicKey
			);

			if (this.includeDelegates) {
				const delegatedAccountsPromise = this.getUserAccountsForDelegate(
					authority ?? this.wallet.publicKey
				);
				[userAccounts, delegatedAccounts] = await Promise.all([
					userAccountsPromise,
					delegatedAccountsPromise,
				]);

				!userAccounts && (userAccounts = []);
				!delegatedAccounts && (delegatedAccounts = []);
			} else {
				userAccounts = (await userAccountsPromise) ?? [];
			}

			const allAccounts = userAccounts.concat(delegatedAccounts);
			const addAllAccountsPromise = allAccounts.map((acc) =>
				this.addUser(acc.subAccountId, acc.authority, acc)
			);

			const addAllAccountsResults = await Promise.all(addAllAccountsPromise);
			result = addAllAccountsResults.every((res) => !!res);

			if (this.activeSubAccountId == undefined) {
				this.switchActiveUser(
					userAccounts.concat(delegatedAccounts)[0]?.subAccountId ?? 0,
					userAccounts.concat(delegatedAccounts)[0]?.authority ?? this.authority
				);
			}
		}

		return result;
	}

	public async initializeUserAccount(
		subAccountId = 0,
		name?: string,
		referrerInfo?: ReferrerInfo,
		txParams?: TxParams
	): Promise<[TransactionSignature, PublicKey]> {
		const initializeIxs = [];

		const [userAccountPublicKey, initializeUserAccountIx] =
			await this.getInitializeUserInstructions(
				subAccountId,
				name,
				referrerInfo
			);

		if (subAccountId === 0) {
			if (
				!(await this.checkIfAccountExists(this.getUserStatsAccountPublicKey()))
			) {
				initializeIxs.push(await this.getInitializeUserStatsIx());
			}
		}

		initializeIxs.push(initializeUserAccountIx);
		const tx = await this.buildTransaction(initializeIxs, txParams);

		const { txSig } = await this.sendTransaction(tx, [], this.opts);

		await this.addUser(subAccountId);

		return [txSig, userAccountPublicKey];
	}

	async getInitializeUserInstructions(
		subAccountId = 0,
		name?: string,
		referrerInfo?: ReferrerInfo
	): Promise<[PublicKey, TransactionInstruction]> {
		const userAccountPublicKey = await getUserAccountPublicKey(
			this.program.programId,
			this.wallet.publicKey,
			subAccountId
		);

		const remainingAccounts = new Array<AccountMeta>();
		if (referrerInfo !== undefined) {
			remainingAccounts.push({
				pubkey: referrerInfo.referrer,
				isWritable: true,
				isSigner: false,
			});
			remainingAccounts.push({
				pubkey: referrerInfo.referrerStats,
				isWritable: true,
				isSigner: false,
			});
		}

		// const state = this.getStateAccount();
		// if (!state.whitelistMint.equals(PublicKey.default)) {
		// 	const associatedTokenPublicKey = await getAssociatedTokenAddress(
		// 		state.whitelistMint,
		// 		this.wallet.publicKey
		// 	);
		// 	remainingAccounts.push({
		// 		pubkey: associatedTokenPublicKey,
		// 		isWritable: false,
		// 		isSigner: false,
		// 	});
		// }

		if (name === undefined) {
			if (subAccountId === 0) {
				name = DEFAULT_USER_NAME;
			} else {
				name = `Subaccount ${subAccountId + 1}`;
			}
		}

		const nameBuffer = encodeName(name);
		const initializeUserAccountIx =
			await this.program.instruction.initializeUser(subAccountId, nameBuffer, {
				accounts: {
					user: userAccountPublicKey,
					userStats: this.getUserStatsAccountPublicKey(),
					authority: this.wallet.publicKey,
					payer: this.wallet.publicKey,
					rent: anchor.web3.SYSVAR_RENT_PUBKEY,
					systemProgram: anchor.web3.SystemProgram.programId,
					state: await this.getStatePublicKey(),
				},
				remainingAccounts,
			});

		return [userAccountPublicKey, initializeUserAccountIx];
	}

	async getInitializeUserStatsIx(): Promise<TransactionInstruction> {
		return await this.program.instruction.initializeUserStats({
			accounts: {
				userStats: this.getUserStatsAccountPublicKey(),
				authority: this.wallet.publicKey,
				payer: this.wallet.publicKey,
				rent: anchor.web3.SYSVAR_RENT_PUBKEY,
				systemProgram: anchor.web3.SystemProgram.programId,
				state: await this.getStatePublicKey(),
			},
		});
	}

	async getNextSubAccountId(): Promise<number> {
		const userStats = this.getUserStats();
		let userStatsAccount: UserStatsAccount;
		if (!userStats) {
			userStatsAccount = await fetchUserStatsAccount(
				this.connection,
				this.program,
				this.wallet.publicKey
			);
		} else {
			userStatsAccount = userStats.getAccount();
		}
		return userStatsAccount.numberOfSubAccountsCreated;
	}

	public async initializeReferrerName(
		name: string
	): Promise<TransactionSignature> {
		const userAccountPublicKey = getUserAccountPublicKeySync(
			this.program.programId,
			this.wallet.publicKey,
			0
		);

		const nameBuffer = encodeName(name);

		const referrerNameAccountPublicKey = getReferrerNamePublicKeySync(
			this.program.programId,
			nameBuffer
		);

		const tx = await this.program.transaction.initializeReferrerName(
			nameBuffer,
			{
				accounts: {
					referrerName: referrerNameAccountPublicKey,
					user: userAccountPublicKey,
					authority: this.wallet.publicKey,
					userStats: this.getUserStatsAccountPublicKey(),
					payer: this.wallet.publicKey,
					rent: anchor.web3.SYSVAR_RENT_PUBKEY,
					systemProgram: anchor.web3.SystemProgram.programId,
				},
			}
		);
		const { txSig } = await this.sendTransaction(tx, [], this.opts);
		return txSig;
	}

	public async updateUserName(
		name: string,
		subAccountId = 0
	): Promise<TransactionSignature> {
		const userAccountPublicKey = getUserAccountPublicKeySync(
			this.program.programId,
			this.wallet.publicKey,
			subAccountId
		);

		const nameBuffer = encodeName(name);
		const tx = await this.program.transaction.updateUserName(
			subAccountId,
			nameBuffer,
			{
				accounts: {
					user: userAccountPublicKey,
					authority: this.wallet.publicKey,
				},
			}
		);
		const { txSig } = await this.sendTransaction(tx, [], this.opts);
		return txSig;
	}

	public async updateUserCustomMarginRatio(
		updates: { marginRatio: number; subAccountId: number }[],
		txParams?: TxParams
	): Promise<TransactionSignature> {
		const ixs = await Promise.all(
			updates.map(async ({ marginRatio, subAccountId }) => {
				const ix = await this.getUpdateUserCustomMarginRatioIx(
					marginRatio,
					subAccountId
				);
				return ix;
			})
		);

		const tx = await this.buildTransaction(ixs, txParams ?? this.txParams);

		const { txSig } = await this.sendTransaction(tx, [], this.opts);
		return txSig;
	}

	public async getUpdateUserCustomMarginRatioIx(
		marginRatio: number,
		subAccountId = 0
	): Promise<TransactionInstruction> {
		const userAccountPublicKey = getUserAccountPublicKeySync(
			this.program.programId,
			this.wallet.publicKey,
			subAccountId
		);

		await this.addUser(subAccountId, this.wallet.publicKey);

		const ix = this.program.instruction.updateUserCustomMarginRatio(
			subAccountId,
			marginRatio,
			{
				accounts: {
					user: userAccountPublicKey,
					authority: this.wallet.publicKey,
				},
			}
		);

		return ix;
	}

	public async getUpdateUserMarginTradingEnabledIx(
		marginTradingEnabled: boolean,
		subAccountId = 0,
		userAccountPublicKey?: PublicKey
	): Promise<TransactionInstruction> {
		const userAccountPublicKeyToUse =
			userAccountPublicKey ||
			getUserAccountPublicKeySync(
				this.program.programId,
				this.wallet.publicKey,
				subAccountId
			);

		await this.addUser(subAccountId, this.wallet.publicKey);

		let remainingAccounts;
		try {
			remainingAccounts = this.getRemainingAccounts({
				userAccounts: [this.getUserAccount(subAccountId)],
			});
		} catch (err) {
			remainingAccounts = [];
		}

		return await this.program.instruction.updateUserMarginTradingEnabled(
			subAccountId,
			marginTradingEnabled,
			{
				accounts: {
					user: userAccountPublicKeyToUse,
					authority: this.wallet.publicKey,
				},
				remainingAccounts,
			}
		);
	}

	public async updateUserMarginTradingEnabled(
		updates: { marginTradingEnabled: boolean; subAccountId: number }[]
	): Promise<TransactionSignature> {
		const ixs = await Promise.all(
			updates.map(async ({ marginTradingEnabled, subAccountId }) => {
				return await this.getUpdateUserMarginTradingEnabledIx(
					marginTradingEnabled,
					subAccountId
				);
			})
		);

		const tx = await this.buildTransaction(ixs, this.txParams);

		const { txSig } = await this.sendTransaction(tx, [], this.opts);
		return txSig;
	}

	public async updateUserDelegate(
		delegate: PublicKey,
		subAccountId = 0
	): Promise<TransactionSignature> {
		const tx = await this.program.transaction.updateUserDelegate(
			subAccountId,
			delegate,
			{
				accounts: {
					user: await this.getUserAccountPublicKey(),
					authority: this.wallet.publicKey,
				},
			}
		);

		const { txSig } = await this.sendTransaction(tx, [], this.opts);
		return txSig;
	}

	public async updateUserReduceOnly(
		updates: { reduceOnly: boolean; subAccountId: number }[]
	): Promise<TransactionSignature> {
		const ixs = await Promise.all(
			updates.map(async ({ reduceOnly, subAccountId }) => {
				return await this.getUpdateUserReduceOnlyIx(reduceOnly, subAccountId);
			})
		);

		const tx = await this.buildTransaction(ixs, this.txParams);

		const { txSig } = await this.sendTransaction(tx, [], this.opts);
		return txSig;
	}

	public async getUpdateUserReduceOnlyIx(
		reduceOnly: boolean,
		subAccountId: number
	) {
		const ix = await this.program.instruction.updateUserReduceOnly(
			subAccountId,
			reduceOnly,
			{
				accounts: {
					user: getUserAccountPublicKeySync(
						this.program.programId,
						this.wallet.publicKey,
						subAccountId
					),
					authority: this.wallet.publicKey,
				},
			}
		);

		return ix;
	}

	public async fetchAllUserAccounts(
		includeIdle = true
	): Promise<ProgramAccount<UserAccount>[]> {
		let filters = undefined;
		if (!includeIdle) {
			filters = [getNonIdleUserFilter()];
		}
		return (await this.program.account.user.all(
			filters
		)) as ProgramAccount<UserAccount>[];
	}

	public async getUserAccountsForDelegate(
		delegate: PublicKey
	): Promise<UserAccount[]> {
		const programAccounts = await this.program.account.user.all([
			{
				memcmp: {
					offset: 40,
					/** data to match, as base-58 encoded string and limited to less than 129 bytes */
					bytes: bs58.encode(delegate.toBuffer()),
				},
			},
		]);

		return programAccounts
			.map((programAccount) => programAccount.account as UserAccount)
			.sort((a, b) => a.subAccountId - b.subAccountId);
	}

	public async getUserAccountsAndAddressesForAuthority(
		authority: PublicKey
	): Promise<ProgramAccount<UserAccount>[]> {
		const programAccounts = await this.program.account.user.all([
			{
				memcmp: {
					offset: 8,
					/** data to match, as base-58 encoded string and limited to less than 129 bytes */
					bytes: bs58.encode(authority.toBuffer()),
				},
			},
		]);

		return programAccounts.map(
			(programAccount) => programAccount as ProgramAccount<UserAccount>
		);
	}

	public async getUserAccountsForAuthority(
		authority: PublicKey
	): Promise<UserAccount[]> {
		const programAccounts = await this.program.account.user.all([
			{
				memcmp: {
					offset: 8,
					/** data to match, as base-58 encoded string and limited to less than 129 bytes */
					bytes: bs58.encode(authority.toBuffer()),
				},
			},
		]);

		return programAccounts
			.map((programAccount) => programAccount.account as UserAccount)
			.sort((a, b) => a.subAccountId - b.subAccountId);
	}

	public async getReferredUserStatsAccountsByReferrer(
		referrer: PublicKey
	): Promise<UserStatsAccount[]> {
		const programAccounts = await this.program.account.userStats.all([
			{
				memcmp: {
					offset: 40,
					/** data to match, as base-58 encoded string and limited to less than 129 bytes */
					bytes: bs58.encode(referrer.toBuffer()),
				},
			},
		]);

		return programAccounts.map(
			(programAccount) => programAccount.account as UserStatsAccount
		);
	}

	public async getReferrerNameAccountsForAuthority(
		authority: PublicKey
	): Promise<ReferrerNameAccount[]> {
		const programAccounts = await this.program.account.referrerName.all([
			{
				memcmp: {
					offset: 8,
					/** data to match, as base-58 encoded string and limited to less than 129 bytes */
					bytes: bs58.encode(authority.toBuffer()),
				},
			},
		]);

		return programAccounts.map(
			(programAccount) => programAccount.account as ReferrerNameAccount
		);
	}

	public async deleteUser(
		subAccountId = 0,
		txParams?: TxParams
	): Promise<TransactionSignature> {
		const userAccountPublicKey = getUserAccountPublicKeySync(
			this.program.programId,
			this.wallet.publicKey,
			subAccountId
		);

		const ix = await this.getUserDeletionIx(userAccountPublicKey);

		const { txSig } = await this.sendTransaction(
			await this.buildTransaction(ix, txParams),
			[],
			this.opts
		);

		const userMapKey = this.getUserMapKey(subAccountId, this.wallet.publicKey);
		await this.users.get(userMapKey)?.unsubscribe();
		this.users.delete(userMapKey);

		return txSig;
	}

	public async getUserDeletionIx(userAccountPublicKey: PublicKey) {
		const ix = await this.program.instruction.deleteUser({
			accounts: {
				user: userAccountPublicKey,
				userStats: this.getUserStatsAccountPublicKey(),
				authority: this.wallet.publicKey,
				state: await this.getStatePublicKey(),
			},
		});

		return ix;
	}

	public async reclaimRent(
		subAccountId = 0,
		txParams?: TxParams
	): Promise<TransactionSignature> {
		const userAccountPublicKey = getUserAccountPublicKeySync(
			this.program.programId,
			this.wallet.publicKey,
			subAccountId
		);

		const ix = await this.getReclaimRentIx(userAccountPublicKey);

		const { txSig } = await this.sendTransaction(
			await this.buildTransaction(ix, txParams),
			[],
			this.opts
		);

		return txSig;
	}

	public async getReclaimRentIx(userAccountPublicKey: PublicKey) {
		return await this.program.instruction.reclaimRent({
			accounts: {
				user: userAccountPublicKey,
				userStats: this.getUserStatsAccountPublicKey(),
				authority: this.wallet.publicKey,
				state: await this.getStatePublicKey(),
				rent: anchor.web3.SYSVAR_RENT_PUBKEY,
			},
		});
	}

	public getUser(subAccountId?: number, authority?: PublicKey): User {
		subAccountId = subAccountId ?? this.activeSubAccountId;
		authority = authority ?? this.authority;
		const userMapKey = this.getUserMapKey(subAccountId, authority);

		if (!this.users.has(userMapKey)) {
			throw new Error(`NormalClient has no user for user id ${userMapKey}`);
		}
		return this.users.get(userMapKey);
	}

	public hasUser(subAccountId?: number, authority?: PublicKey): boolean {
		subAccountId = subAccountId ?? this.activeSubAccountId;
		authority = authority ?? this.authority;
		const userMapKey = this.getUserMapKey(subAccountId, authority);

		return this.users.has(userMapKey);
	}

	public getUsers(): User[] {
		// delegate users get added to the end
		return [...this.users.values()]
			.filter((acct) =>
				acct.getUserAccount().authority.equals(this.wallet.publicKey)
			)
			.concat(
				[...this.users.values()].filter(
					(acct) =>
						!acct.getUserAccount().authority.equals(this.wallet.publicKey)
				)
			);
	}

	public getUserStats(): UserStats {
		return this.userStats;
	}

	public async fetchReferrerNameAccount(
		name: string
	): Promise<ReferrerNameAccount | undefined> {
		const nameBuffer = encodeName(name);
		const referrerNameAccountPublicKey = getReferrerNamePublicKeySync(
			this.program.programId,
			nameBuffer
		);
		return (await this.program.account.referrerName.fetch(
			referrerNameAccountPublicKey
		)) as ReferrerNameAccount;
	}

	userStatsAccountPublicKey: PublicKey;
	public getUserStatsAccountPublicKey(): PublicKey {
		if (this.userStatsAccountPublicKey) {
			return this.userStatsAccountPublicKey;
		}

		this.userStatsAccountPublicKey = getUserStatsAccountPublicKey(
			this.program.programId,
			this.authority
		);
		return this.userStatsAccountPublicKey;
	}

	public async getUserAccountPublicKey(
		subAccountId?: number,
		authority?: PublicKey
	): Promise<PublicKey> {
		return this.getUser(subAccountId, authority).userAccountPublicKey;
	}

	public getUserAccount(
		subAccountId?: number,
		authority?: PublicKey
	): UserAccount | undefined {
		return this.getUser(subAccountId, authority).getUserAccount();
	}

	/**
	 * Forces a fetch to rpc before returning accounts. Useful for anchor tests.
	 * @param subAccountId
	 */
	public async forceGetUserAccount(
		subAccountId?: number
	): Promise<UserAccount | undefined> {
		await this.getUser(subAccountId).fetchAccounts();
		return this.getUser(subAccountId).getUserAccount();
	}

	public getUserAccountAndSlot(
		subAccountId?: number
	): DataAndSlot<UserAccount> | undefined {
		return this.getUser(subAccountId).getUserAccountAndSlot();
	}

	/**
	 * Converts an amount to the price precision. The perp market precision is {@link PRICE_PRECISION} (1e6).
	 * @param amount
	 */
	public convertToPricePrecision(amount: BN | number): BN {
		if (typeof amount === 'number') {
			return numberToSafeBN(amount, PRICE_PRECISION);
		} else {
			return amount.mul(BASE_PRECISION);
		}
	}

	getRemainingAccounts(params: RemainingAccountParams): AccountMeta[] {
		const { oracleAccountMap, marketAccountMap } =
			this.getRemainingAccountMapsForUsers(params.userAccounts);

		if (params.useMarketLastSlotCache) {
			const lastUserSlot = this.getUserAccountAndSlot()?.slot;

			for (const [marketIndex, slot] of this.marketLastSlotCache.entries()) {
				// if cache has more recent slot than user positions account slot, add market to remaining accounts
				// otherwise remove from slot
				if (slot > lastUserSlot) {
					this.addMarketToRemainingAccountMaps(
						marketIndex,
						false,
						oracleAccountMap,
						marketAccountMap
					);
				} else {
					this.marketLastSlotCache.delete(marketIndex);
				}
			}
		}

		if (params.readableMarketIndex !== undefined) {
			const readableMarketIndexes = Array.isArray(params.readableMarketIndex)
				? params.readableMarketIndex
				: [params.readableMarketIndex];
			for (const marketIndex of readableMarketIndexes) {
				this.addMarketToRemainingAccountMaps(
					marketIndex,
					false,
					oracleAccountMap,
					marketAccountMap
				);
			}
		}

		for (const marketIndex of this.mustIncludeMarketIndexes.values()) {
			this.addMarketToRemainingAccountMaps(
				marketIndex,
				false,
				oracleAccountMap,
				marketAccountMap
			);
		}

		if (params.writableMarketIndexes !== undefined) {
			for (const writableMarketIndex of params.writableMarketIndexes) {
				this.addMarketToRemainingAccountMaps(
					writableMarketIndex,
					true,
					oracleAccountMap,
					marketAccountMap
				);
			}
		}

		return [
			...oracleAccountMap.values(),
			// ...marketAccountMap.values(),
		];
	}

	addMarketToRemainingAccountMaps(
		marketIndex: number,
		writable: boolean,
		oracleAccountMap: Map<string, AccountMeta>,
		marketAccountMap: Map<number, AccountMeta>
	): void {
		const marketAccount = this.getMarketAccount(marketIndex);
		marketAccountMap.set(marketIndex, {
			pubkey: marketAccount.pubkey,
			isSigner: false,
			isWritable: writable,
		});
		const oracleWritable =
			writable && isVariant(marketAccount.amm.oracleSource, 'prelaunch');
		oracleAccountMap.set(marketAccount.amm.oracle.toString(), {
			pubkey: marketAccount.amm.oracle,
			isSigner: false,
			isWritable: oracleWritable,
		});
	}

	getRemainingAccountMapsForUsers(userAccounts: UserAccount[]): {
		oracleAccountMap: Map<string, AccountMeta>;
		marketAccountMap: Map<number, AccountMeta>;
	} {
		const oracleAccountMap = new Map<string, AccountMeta>();
		const marketAccountMap = new Map<number, AccountMeta>();

		for (const userAccount of userAccounts) {
			// for (const position of userAccount.perpPositions) {
			// 	if (!positionIsAvailable(position)) {
			// 		this.addPerpMarketToRemainingAccountMaps(
			// 			position.marketIndex,
			// 			false,
			// 			oracleAccountMap,
			// 			spotMarketAccountMap,
			// 			perpMarketAccountMap
			// 		);
			// 	}
			// }
		}

		return {
			oracleAccountMap,
			marketAccountMap,
		};
	}

	/**
	 * Get the associated token address for the given market
	 * @param marketIndex
	 * @param useNative
	 * @param tokenProgram
	 */
	public async getAssociatedTokenAccount(
		marketIndex: number,
		useNative = true,
		tokenProgram = TOKEN_PROGRAM_ID
	): Promise<PublicKey> {
		const market = this.getMarketAccount(marketIndex);
		if (useNative && market.mint.equals(WRAPPED_SOL_MINT)) {
			return this.wallet.publicKey;
		}
		const mint = market.mint;
		return await getAssociatedTokenAddress(
			mint,
			this.wallet.publicKey,
			undefined,
			tokenProgram
		);
	}

	public createAssociatedTokenAccountIdempotentInstruction(
		account: PublicKey,
		payer: PublicKey,
		owner: PublicKey,
		mint: PublicKey,
		tokenProgram = TOKEN_PROGRAM_ID
	): TransactionInstruction {
		return new TransactionInstruction({
			keys: [
				{ pubkey: payer, isSigner: true, isWritable: true },
				{ pubkey: account, isSigner: false, isWritable: true },
				{ pubkey: owner, isSigner: false, isWritable: false },
				{ pubkey: mint, isSigner: false, isWritable: false },
				{
					pubkey: anchor.web3.SystemProgram.programId,
					isSigner: false,
					isWritable: false,
				},
				{ pubkey: tokenProgram, isSigner: false, isWritable: false },
			],
			programId: ASSOCIATED_TOKEN_PROGRAM_ID,
			data: Buffer.from([0x1]),
		});
	}

	private async checkIfAccountExists(account: PublicKey): Promise<boolean> {
		try {
			const accountInfo = await this.connection.getAccountInfo(account);
			return accountInfo != null;
		} catch (e) {
			// Doesn't already exist
			return false;
		}
	}

	public async getWrappedSolAccountCreationIxs(
		amount: BN,
		includeRent?: boolean
	): Promise<{
		ixs: anchor.web3.TransactionInstruction[];
		/** @deprecated - this array is always going to be empty, in the current implementation */
		signers: Signer[];
		pubkey: PublicKey;
	}> {
		const authority = this.wallet.publicKey;

		// Generate a random seed for wrappedSolAccount.
		const seed = Keypair.generate().publicKey.toBase58().slice(0, 32);

		// Calculate a publicKey that will be controlled by the authority.
		const wrappedSolAccount = await PublicKey.createWithSeed(
			authority,
			seed,
			TOKEN_PROGRAM_ID
		);

		const result = {
			ixs: [],
			signers: [],
			pubkey: wrappedSolAccount,
		};

		const rentSpaceLamports = new BN(LAMPORTS_PER_SOL / 100);

		const lamports = includeRent
			? amount.add(rentSpaceLamports)
			: rentSpaceLamports;

		result.ixs.push(
			SystemProgram.createAccountWithSeed({
				fromPubkey: authority,
				basePubkey: authority,
				seed,
				newAccountPubkey: wrappedSolAccount,
				lamports: lamports.toNumber(),
				space: 165,
				programId: TOKEN_PROGRAM_ID,
			})
		);

		result.ixs.push(
			createInitializeAccountInstruction(
				wrappedSolAccount,
				WRAPPED_SOL_MINT,
				authority
			)
		);

		return result;
	}

	public getTokenProgramForMarket(marketAccount: MarketAccount): PublicKey {
		if (marketAccount.tokenProgram === 1) {
			return TOKEN_2022_PROGRAM_ID;
		}
		return TOKEN_PROGRAM_ID;
	}

	public getTokenProgramForInsuranceFund(
		insuranceFundAccount: InsuranceFundAccount
	): PublicKey {
		if (insuranceFundAccount.tokenProgram === 1) {
			return TOKEN_2022_PROGRAM_ID;
		}
		return TOKEN_PROGRAM_ID;
	}

	public addTokenMintToRemainingAccounts(
		insuranceFundAccount: InsuranceFundAccount,
		remainingAccounts: AccountMeta[]
	) {
		if (insuranceFundAccount.tokenProgram === 1) {
			remainingAccounts.push({
				pubkey: insuranceFundAccount.mint,
				isSigner: false,
				isWritable: false,
			});
		}
	}

	public getAssociatedTokenAccountCreationIx(
		tokenMintAddress: PublicKey,
		associatedTokenAddress: PublicKey,
		tokenProgram: PublicKey
	): anchor.web3.TransactionInstruction {
		return createAssociatedTokenAccountInstruction(
			this.wallet.publicKey,
			associatedTokenAddress,
			this.wallet.publicKey,
			tokenMintAddress,
			tokenProgram
		);
	}

	public async initializeUserAccountForDevnet(
		subAccountId = 0,
		name = DEFAULT_USER_NAME,
		marketIndex: number,
		tokenFaucet: TokenFaucet,
		amount: BN,
		referrerInfo?: ReferrerInfo,
		txParams?: TxParams
	): Promise<[TransactionSignature, PublicKey]> {
		const ixs = [];

		const [associateTokenPublicKey, createAssociatedAccountIx, mintToIx] =
			await tokenFaucet.createAssociatedTokenAccountAndMintToInstructions(
				this.wallet.publicKey,
				amount
			);

		const [userAccountPublicKey, initializeUserAccountIx] =
			await this.getInitializeUserInstructions(
				subAccountId,
				name,
				referrerInfo
			);

		// const depositCollateralIx = await this.getDepositInstruction(
		// 	amount,
		// 	marketIndex,
		// 	associateTokenPublicKey,
		// 	subAccountId,
		// 	false,
		// 	false
		// );

		ixs.push(createAssociatedAccountIx, mintToIx);

		if (subAccountId === 0) {
			if (
				!(await this.checkIfAccountExists(this.getUserStatsAccountPublicKey()))
			) {
				ixs.push(await this.getInitializeUserStatsIx());
			}
		}
		// ixs.push(initializeUserAccountIx, depositCollateralIx);
		ixs.push(initializeUserAccountIx);

		const tx = await this.buildTransaction(ixs, txParams);

		const { txSig } = await this.sendTransaction(tx, [], this.opts);

		await this.addUser(subAccountId);

		return [txSig, userAccountPublicKey];
	}

	public async updateUserIdle(
		userAccountPublicKey: PublicKey,
		user: UserAccount,
		txParams?: TxParams,
		fillerPublicKey?: PublicKey
	): Promise<TransactionSignature> {
		const { txSig } = await this.sendTransaction(
			await this.buildTransaction(
				await this.getUpdateUserIdleIx(
					userAccountPublicKey,
					user,
					fillerPublicKey
				),
				txParams
			),
			[],
			this.opts
		);
		return txSig;
	}

	public async getUpdateUserIdleIx(
		userAccountPublicKey: PublicKey,
		userAccount: UserAccount,
		fillerPublicKey?: PublicKey
	): Promise<TransactionInstruction> {
		const filler = fillerPublicKey ?? (await this.getUserAccountPublicKey());

		const remainingAccounts = this.getRemainingAccounts({
			userAccounts: [userAccount],
		});

		return await this.program.instruction.updateUserIdle({
			accounts: {
				state: await this.getStatePublicKey(),
				filler,
				user: userAccountPublicKey,
				authority: this.wallet.publicKey,
			},
			remainingAccounts,
		});
	}

	public async signMessage(
		message: Uint8Array,
		keypair: Keypair = this.wallet.payer
	): Promise<Buffer> {
		return Buffer.from(await ed.sign(message, keypair.secretKey.slice(0, 32)));
	}

	public async getSetUserStatusToBeingLiquidatedIx(
		userAccountPublicKey: PublicKey,
		userAccount: UserAccount
	): Promise<TransactionInstruction> {
		const remainingAccounts = this.getRemainingAccounts({
			userAccounts: [userAccount],
		});
		return await this.program.instruction.setUserStatusToBeingLiquidated({
			accounts: {
				state: await this.getStatePublicKey(),
				user: userAccountPublicKey,
				authority: this.wallet.publicKey,
			},
			remainingAccounts,
		});
	}

	public async setUserStatusToBeingLiquidated(
		userAccountPublicKey: PublicKey,
		userAccount: UserAccount
	): Promise<TransactionSignature> {
		const { txSig } = await this.sendTransaction(
			await this.buildTransaction(
				await this.getSetUserStatusToBeingLiquidatedIx(
					userAccountPublicKey,
					userAccount
				)
			),
			[],
			this.opts
		);
		return txSig;
	}

	public triggerEvent(eventName: keyof NormalClientAccountEvents, data?: any) {
		this.eventEmitter.emit(eventName, data);
	}

	public getOracleDataForMarket(marketIndex: number): OraclePriceData {
		return this.accountSubscriber.getOraclePriceDataAndSlotForMarket(
			marketIndex
		).data;
	}

	public async initializeInsuranceFundStake(
		txParams?: TxParams
	): Promise<TransactionSignature> {
		const { txSig } = await this.sendTransaction(
			await this.buildTransaction(
				await this.getInitializeInsuranceFundStakeIx(),
				txParams
			),
			[],
			this.opts
		);
		return txSig;
	}

	public async getInitializeInsuranceFundStakeIx(): Promise<TransactionInstruction> {
		const ifStakeAccountPublicKey = getInsuranceFundStakeAccountPublicKey(
			this.program.programId,
			this.wallet.publicKey
		);

		return await this.program.instruction.initializeInsuranceFundStake({
			accounts: {
				insuranceFundStake: ifStakeAccountPublicKey,
				insuranceFund: this.getInsuranceFundAccount().pubkey,
				userStats: this.getUserStatsAccountPublicKey(),
				authority: this.wallet.publicKey,
				payer: this.wallet.publicKey,
				rent: anchor.web3.SYSVAR_RENT_PUBKEY,
				systemProgram: anchor.web3.SystemProgram.programId,
				state: await this.getStatePublicKey(),
			},
		});
	}

	public async getAddInsuranceFundStakeIx(
		amount: BN,
		collateralAccountPublicKey: PublicKey
	): Promise<TransactionInstruction> {
		const insuranceFund = this.getInsuranceFundAccount();
		const ifStakeAccountPublicKey = getInsuranceFundStakeAccountPublicKey(
			this.program.programId,
			this.wallet.publicKey
		);

		const remainingAccounts = [];
		this.addTokenMintToRemainingAccounts(insuranceFund, remainingAccounts);
		const tokenProgram = this.getTokenProgramForInsuranceFund(insuranceFund);
		const ix = this.program.instruction.addInsuranceFundStake(amount, {
			accounts: {
				state: await this.getStatePublicKey(),
				insuranceFund: insuranceFund.pubkey,
				insuranceFundStake: ifStakeAccountPublicKey,
				userStats: this.getUserStatsAccountPublicKey(),
				authority: this.wallet.publicKey,
				insuranceFundVault: insuranceFund.vault,
				normalSigner: this.getSignerPublicKey(),
				userTokenAccount: collateralAccountPublicKey,
				tokenProgram,
			},
			remainingAccounts,
		});

		return ix;
	}

	/**
	 * Add to an insurance fund stake and optionally initialize the account
	 */
	public async addInsuranceFundStake({
		amount,
		collateralAccountPublicKey,
		initializeStakeAccount,
		fromSubaccount,
		txParams,
	}: {
		amount: BN;
		/**
		 * The account where the funds to stake come from. Usually an associated token account
		 */
		collateralAccountPublicKey: PublicKey;
		/**
		 * Add instructions to initialize the staking account -- required if its the first time the currrent authority has staked in this market
		 */
		initializeStakeAccount?: boolean;
		/**
		 * Optional -- withdraw from current subaccount to fund stake amount, instead of wallet balance
		 */
		fromSubaccount?: boolean;
		txParams?: TxParams;
	}): Promise<TransactionSignature> {
		const addIfStakeIxs = [];

		const additionalSigners: Array<Signer> = [];
		const insuranceFundAccount = this.getInsuranceFundAccount();
		const isSolMarket = insuranceFundAccount.mint.equals(WRAPPED_SOL_MINT);
		const createWSOLTokenAccount =
			isSolMarket && collateralAccountPublicKey.equals(this.wallet.publicKey);

		let tokenAccount;

		if (
			!(await this.checkIfAccountExists(this.getUserStatsAccountPublicKey()))
		) {
			addIfStakeIxs.push(await this.getInitializeUserStatsIx());
		}

		if (createWSOLTokenAccount) {
			const { ixs, pubkey } = await this.getWrappedSolAccountCreationIxs(
				amount,
				true
			);
			tokenAccount = pubkey;
			ixs.forEach((ix) => {
				addIfStakeIxs.push(ix);
			});
		} else {
			tokenAccount = collateralAccountPublicKey;
		}

		if (fromSubaccount) {
			// const withdrawIx = await this.getWithdrawIx(
			// 	amount,
			// 	marketIndex,
			// 	tokenAccount
			// );
			// addIfStakeIxs.push(withdrawIx);
		}

		if (initializeStakeAccount) {
			const initializeIx = await this.getInitializeInsuranceFundStakeIx();
			addIfStakeIxs.push(initializeIx);
		}

		const addFundsIx = await this.getAddInsuranceFundStakeIx(
			amount,
			tokenAccount
		);

		addIfStakeIxs.push(addFundsIx);

		if (createWSOLTokenAccount) {
			addIfStakeIxs.push(
				createCloseAccountInstruction(
					tokenAccount,
					this.wallet.publicKey,
					this.wallet.publicKey,
					[]
				)
			);
		}

		const tx = await this.buildTransaction(addIfStakeIxs, txParams);

		const { txSig } = await this.sendTransaction(
			tx,
			additionalSigners,
			this.opts
		);

		return txSig;
	}

	public async requestRemoveInsuranceFundStake(
		amount: BN,
		txParams?: TxParams
	): Promise<TransactionSignature> {
		const insuranceFundAccount = this.getInsuranceFundAccount();
		const ifStakeAccountPublicKey = getInsuranceFundStakeAccountPublicKey(
			this.program.programId,
			this.wallet.publicKey
		);

		const ix = await this.program.instruction.requestRemoveInsuranceFundStake(
			amount,
			{
				accounts: {
					state: await this.getStatePublicKey(),
					insuranceFund: insuranceFundAccount.pubkey,
					insuranceFundStake: ifStakeAccountPublicKey,
					userStats: this.getUserStatsAccountPublicKey(),
					authority: this.wallet.publicKey,
					insuranceFundVault: insuranceFundAccount.vault,
				},
			}
		);

		const tx = await this.buildTransaction(ix, txParams);

		const { txSig } = await this.sendTransaction(tx, [], this.opts);
		return txSig;
	}

	public async cancelRequestRemoveInsuranceFundStake(
		txParams?: TxParams
	): Promise<TransactionSignature> {
		const insuranceFundAccount = this.getInsuranceFundAccount();
		const ifStakeAccountPublicKey = getInsuranceFundStakeAccountPublicKey(
			this.program.programId,
			this.wallet.publicKey
		);

		const ix =
			await this.program.instruction.cancelRequestRemoveInsuranceFundStake({
				accounts: {
					state: await this.getStatePublicKey(),
					insuranceFund: insuranceFundAccount.pubkey,
					insuranceFundStake: ifStakeAccountPublicKey,
					userStats: this.getUserStatsAccountPublicKey(),
					authority: this.wallet.publicKey,
					insuranceFundVault: insuranceFundAccount.vault,
				},
			});

		const tx = await this.buildTransaction(ix, txParams);

		const { txSig } = await this.sendTransaction(tx, [], this.opts);
		return txSig;
	}

	public async removeInsuranceFundStake(
		collateralAccountPublicKey: PublicKey,
		txParams?: TxParams
	): Promise<TransactionSignature> {
		const removeIfStakeIxs = [];
		const insuranceFundAccount = this.getInsuranceFundAccount();
		const ifStakeAccountPublicKey = getInsuranceFundStakeAccountPublicKey(
			this.program.programId,
			this.wallet.publicKey
		);

		const additionalSigners: Array<Signer> = [];
		const isSolMarket = insuranceFundAccount.mint.equals(WRAPPED_SOL_MINT);
		const createWSOLTokenAccount =
			isSolMarket && collateralAccountPublicKey.equals(this.wallet.publicKey);

		let tokenAccount;

		if (createWSOLTokenAccount) {
			const { ixs, pubkey } = await this.getWrappedSolAccountCreationIxs(
				ZERO,
				true
			);
			tokenAccount = pubkey;
			ixs.forEach((ix) => {
				removeIfStakeIxs.push(ix);
			});
		} else {
			tokenAccount = collateralAccountPublicKey;
			const tokenAccountExists = await this.checkIfAccountExists(tokenAccount);
			if (!tokenAccountExists) {
				const createTokenAccountIx =
					await this.createAssociatedTokenAccountIdempotentInstruction(
						tokenAccount,
						this.wallet.publicKey,
						this.wallet.publicKey,
						insuranceFundAccount.mint
					);
				removeIfStakeIxs.push(createTokenAccountIx);
			}
		}

		const remainingAccounts = [];
		this.addTokenMintToRemainingAccounts(
			insuranceFundAccount,
			remainingAccounts
		);
		const tokenProgram =
			this.getTokenProgramForInsuranceFund(insuranceFundAccount);
		const removeStakeIx =
			await this.program.instruction.removeInsuranceFundStake({
				accounts: {
					state: await this.getStatePublicKey(),
					insuranceFund: insuranceFundAccount.pubkey,
					insuranceFundStake: ifStakeAccountPublicKey,
					userStats: this.getUserStatsAccountPublicKey(),
					authority: this.wallet.publicKey,
					insuranceFundVault: insuranceFundAccount.vault,
					normalSigner: this.getSignerPublicKey(),
					userTokenAccount: tokenAccount,
					tokenProgram,
				},
				remainingAccounts,
			});

		removeIfStakeIxs.push(removeStakeIx);

		// Close the wrapped sol account at the end of the transaction
		if (createWSOLTokenAccount) {
			removeIfStakeIxs.push(
				createCloseAccountInstruction(
					tokenAccount,
					this.wallet.publicKey,
					this.wallet.publicKey,
					[]
				)
			);
		}

		const tx = await this.buildTransaction(removeIfStakeIxs, txParams);

		const { txSig } = await this.sendTransaction(
			tx,
			additionalSigners,
			this.opts
		);
		return txSig;
	}

	public async updateUserQuoteAssetInsuranceStake(
		authority: PublicKey,
		txParams?: TxParams
	): Promise<TransactionSignature> {
		const tx = await this.buildTransaction(
			await this.getUpdateUserQuoteAssetInsuranceStakeIx(authority),
			txParams
		);
		const { txSig } = await this.sendTransaction(tx, [], this.opts);
		return txSig;
	}

	public async getUpdateUserQuoteAssetInsuranceStakeIx(
		authority: PublicKey
	): Promise<TransactionInstruction> {
		const marketIndex = QUOTE_SPOT_MARKET_INDEX;
		const insuranceFund = this.getInsuranceFundAccount();
		const ifStakeAccountPublicKey = getInsuranceFundStakeAccountPublicKey(
			this.program.programId,
			authority
		);
		const userStatsPublicKey = getUserStatsAccountPublicKey(
			this.program.programId,
			authority
		);

		const ix = this.program.instruction.updateUserQuoteAssetInsuranceStake({
			accounts: {
				state: await this.getStatePublicKey(),
				insuranceFund: insuranceFund.pubkey,
				insuranceFundStake: ifStakeAccountPublicKey,
				userStats: userStatsPublicKey,
				signer: this.wallet.publicKey,
				insuranceFundVault: insuranceFund.vault,
			},
		});

		return ix;
	}

	public getReceiverProgram(): Program<PythSolanaReceiver> {
		if (this.receiverProgram === undefined) {
			this.receiverProgram = new Program(
				pythSolanaReceiverIdl as PythSolanaReceiver,
				DEFAULT_RECEIVER_PROGRAM_ID,
				this.provider
			);
		}
		return this.receiverProgram;
	}

	public async postPythPullOracleUpdateAtomic(
		vaaString: string,
		feedId: string
	): Promise<TransactionSignature> {
		const postIxs = await this.getPostPythPullOracleUpdateAtomicIxs(
			vaaString,
			feedId
		);
		const tx = await this.buildTransaction(postIxs);
		const { txSig } = await this.sendTransaction(tx, [], this.opts);

		return txSig;
	}

	public async postMultiPythPullOracleUpdatesAtomic(
		vaaString: string,
		feedIds: string[]
	): Promise<TransactionSignature> {
		const postIxs = await this.getPostPythPullOracleUpdateAtomicIxs(
			vaaString,
			feedIds
		);
		const tx = await this.buildTransaction(postIxs);
		const { txSig } = await this.sendTransaction(tx, [], this.opts);

		return txSig;
	}

	public async getPostPythPullOracleUpdateAtomicIxs(
		vaaString: string,
		feedIds: string | string[],
		numSignatures = 2
	): Promise<TransactionInstruction[]> {
		const accumulatorUpdateData = parseAccumulatorUpdateData(
			Buffer.from(vaaString, 'base64')
		);
		const guardianSetIndex = accumulatorUpdateData.vaa.readUInt32BE(1);
		const guardianSet = getGuardianSetPda(
			guardianSetIndex,
			DEFAULT_WORMHOLE_PROGRAM_ID
		);
		const trimmedVaa = trimVaaSignatures(
			accumulatorUpdateData.vaa,
			numSignatures
		);

		const postIxs: TransactionInstruction[] = [];
		if (accumulatorUpdateData.updates.length > 1) {
			const encodedParams = this.getReceiverProgram().coder.types.encode(
				'PostMultiUpdatesAtomicParams',
				{
					vaa: trimmedVaa,
					merklePriceUpdates: accumulatorUpdateData.updates,
				}
			);
			const feedIdsToUse: string[] =
				typeof feedIds === 'string' ? [feedIds] : feedIds;
			const pubkeys = feedIdsToUse.map((feedId) => {
				return getPythPullOraclePublicKey(
					this.program.programId,
					getFeedIdUint8Array(feedId)
				);
			});

			const remainingAccounts: Array<AccountMeta> = pubkeys.map((pubkey) => {
				return {
					pubkey,
					isSigner: false,
					isWritable: true,
				};
			});
			postIxs.push(
				this.program.instruction.postMultiPythPullOracleUpdatesAtomic(
					encodedParams,
					{
						accounts: {
							keeper: this.wallet.publicKey,
							pythSolanaReceiver: NORMAL_ORACLE_RECEIVER_ID,
							guardianSet,
						},
						remainingAccounts,
					}
				)
			);
		} else {
			let feedIdToUse = typeof feedIds === 'string' ? feedIds : feedIds[0];
			feedIdToUse = trimFeedId(feedIdToUse);
			postIxs.push(
				await this.getSinglePostPythPullOracleAtomicIx(
					{
						vaa: trimmedVaa,
						merklePriceUpdate: accumulatorUpdateData.updates[0],
					},
					feedIdToUse,
					guardianSet
				)
			);
		}
		return postIxs;
	}

	private async getSinglePostPythPullOracleAtomicIx(
		params: {
			vaa: Buffer;
			merklePriceUpdate: {
				message: Buffer;
				proof: number[][];
			};
		},
		feedId: string,
		guardianSet: PublicKey
	): Promise<TransactionInstruction> {
		const feedIdBuffer = getFeedIdUint8Array(feedId);
		const receiverProgram = this.getReceiverProgram();

		const encodedParams = receiverProgram.coder.types.encode(
			'PostUpdateAtomicParams',
			params
		);

		return this.program.instruction.postPythPullOracleUpdateAtomic(
			feedIdBuffer,
			encodedParams,
			{
				accounts: {
					keeper: this.wallet.publicKey,
					pythSolanaReceiver: NORMAL_ORACLE_RECEIVER_ID,
					guardianSet,
					priceFeed: getPythPullOraclePublicKey(
						this.program.programId,
						feedIdBuffer
					),
				},
			}
		);
	}

	public async updatePythPullOracle(
		vaaString: string,
		feedId: string
	): Promise<TransactionSignature> {
		feedId = trimFeedId(feedId);
		const accumulatorUpdateData = parseAccumulatorUpdateData(
			Buffer.from(vaaString, 'base64')
		);
		const guardianSetIndex = accumulatorUpdateData.vaa.readUInt32BE(1);
		const guardianSet = getGuardianSetPda(
			guardianSetIndex,
			DEFAULT_WORMHOLE_PROGRAM_ID
		);

		const [postIxs, encodedVaaAddress] = await this.getBuildEncodedVaaIxs(
			accumulatorUpdateData.vaa,
			guardianSet
		);

		for (const update of accumulatorUpdateData.updates) {
			postIxs.push(
				await this.getUpdatePythPullOracleIxs(
					{
						merklePriceUpdate: update,
					},
					feedId,
					encodedVaaAddress.publicKey
				)
			);
		}

		const tx = await this.buildTransaction(postIxs);
		const { txSig } = await this.sendTransaction(
			tx,
			[encodedVaaAddress],
			this.opts
		);

		return txSig;
	}

	public async getUpdatePythPullOracleIxs(
		params: {
			merklePriceUpdate: {
				message: Buffer;
				proof: number[][];
			};
		},
		feedId: string,
		encodedVaaAddress: PublicKey
	): Promise<TransactionInstruction> {
		const feedIdBuffer = getFeedIdUint8Array(feedId);
		const receiverProgram = this.getReceiverProgram();

		const encodedParams = receiverProgram.coder.types.encode(
			'PostUpdateParams',
			params
		);

		return this.program.instruction.updatePythPullOracle(
			feedIdBuffer,
			encodedParams,
			{
				accounts: {
					keeper: this.wallet.publicKey,
					pythSolanaReceiver: NORMAL_ORACLE_RECEIVER_ID,
					encodedVaa: encodedVaaAddress,
					priceFeed: getPythPullOraclePublicKey(
						this.program.programId,
						feedIdBuffer
					),
				},
			}
		);
	}

	private async getBuildEncodedVaaIxs(
		vaa: Buffer,
		guardianSet: PublicKey
	): Promise<[TransactionInstruction[], Keypair]> {
		const postIxs: TransactionInstruction[] = [];

		if (this.wormholeProgram === undefined) {
			this.wormholeProgram = new Program(
				wormholeCoreBridgeIdl,
				DEFAULT_WORMHOLE_PROGRAM_ID,
				this.provider
			);
		}

		const encodedVaaKeypair = new Keypair();
		postIxs.push(
			await this.wormholeProgram.account.encodedVaa.createInstruction(
				encodedVaaKeypair,
				vaa.length + 46
			)
		);

		// Why do we need this too?
		postIxs.push(
			await this.wormholeProgram.methods
				.initEncodedVaa()
				.accounts({
					encodedVaa: encodedVaaKeypair.publicKey,
				})
				.instruction()
		);

		// Split the write into two ixs
		postIxs.push(
			await this.wormholeProgram.methods
				.writeEncodedVaa({
					index: 0,
					data: vaa.subarray(0, 755),
				})
				.accounts({
					draftVaa: encodedVaaKeypair.publicKey,
				})
				.instruction()
		);

		postIxs.push(
			await this.wormholeProgram.methods
				.writeEncodedVaa({
					index: 755,
					data: vaa.subarray(755),
				})
				.accounts({
					draftVaa: encodedVaaKeypair.publicKey,
				})
				.instruction()
		);

		// Verify
		postIxs.push(
			await this.wormholeProgram.methods
				.verifyEncodedVaaV1()
				.accounts({
					guardianSet,
					draftVaa: encodedVaaKeypair.publicKey,
				})
				.instruction()
		);

		return [postIxs, encodedVaaKeypair];
	}

	private handleSignedTransaction(signedTxs: SignedTxData[]) {
		if (this.enableMetricsEvents && this.metricsEventEmitter) {
			this.metricsEventEmitter.emit('txSigned', signedTxs);
		}
	}

	private handlePreSignedTransaction() {
		if (this.enableMetricsEvents && this.metricsEventEmitter) {
			this.metricsEventEmitter.emit('preTxSigned');
		}
	}

	private isVersionedTransaction(
		tx: Transaction | VersionedTransaction
	): boolean {
		return isVersionedTransaction(tx);
	}

	sendTransaction(
		tx: Transaction | VersionedTransaction,
		additionalSigners?: Array<Signer>,
		opts?: ConfirmOptions,
		preSigned?: boolean
	): Promise<TxSigAndSlot> {
		const isVersionedTx = this.isVersionedTransaction(tx);

		if (isVersionedTx) {
			return this.txSender.sendVersionedTransaction(
				tx as VersionedTransaction,
				additionalSigners,
				opts,
				preSigned
			);
		} else {
			return this.txSender.send(
				tx as Transaction,
				additionalSigners,
				opts,
				preSigned
			);
		}
	}

	async buildTransaction(
		instructions: TransactionInstruction | TransactionInstruction[],
		txParams?: TxParams,
		txVersion?: TransactionVersion,
		lookupTables?: AddressLookupTableAccount[],
		forceVersionedTransaction?: boolean,
		recentBlockhash?: BlockhashWithExpiryBlockHeight
	): Promise<Transaction | VersionedTransaction> {
		return this.txHandler.buildTransaction({
			instructions,
			txVersion: txVersion ?? this.txVersion,
			txParams: txParams ?? this.txParams,
			connection: this.connection,
			preFlightCommitment: this.opts.preflightCommitment,
			fetchMarketLookupTableAccount:
				this.fetchMarketLookupTableAccount.bind(this),
			lookupTables,
			forceVersionedTransaction,
			recentBlockhash,
		});
	}

	async buildBulkTransactions(
		instructions: (TransactionInstruction | TransactionInstruction[])[],
		txParams?: TxParams,
		txVersion?: TransactionVersion,
		lookupTables?: AddressLookupTableAccount[],
		forceVersionedTransaction?: boolean
	): Promise<(Transaction | VersionedTransaction)[]> {
		return this.txHandler.buildBulkTransactions({
			instructions,
			txVersion: txVersion ?? this.txVersion,
			txParams: txParams ?? this.txParams,
			connection: this.connection,
			preFlightCommitment: this.opts.preflightCommitment,
			fetchMarketLookupTableAccount:
				this.fetchMarketLookupTableAccount.bind(this),
			lookupTables,
			forceVersionedTransaction,
		});
	}

	async buildTransactionsMap(
		instructionsMap: Record<
			string,
			TransactionInstruction | TransactionInstruction[]
		>,
		txParams?: TxParams,
		txVersion?: TransactionVersion,
		lookupTables?: AddressLookupTableAccount[],
		forceVersionedTransaction?: boolean
	) {
		return this.txHandler.buildTransactionsMap({
			instructionsMap,
			txVersion: txVersion ?? this.txVersion,
			txParams: txParams ?? this.txParams,
			connection: this.connection,
			preFlightCommitment: this.opts.preflightCommitment,
			fetchMarketLookupTableAccount:
				this.fetchMarketLookupTableAccount.bind(this),
			lookupTables,
			forceVersionedTransaction,
		});
	}

	async buildAndSignTransactionsMap(
		instructionsMap: Record<
			string,
			TransactionInstruction | TransactionInstruction[]
		>,
		txParams?: TxParams,
		txVersion?: TransactionVersion,
		lookupTables?: AddressLookupTableAccount[],
		forceVersionedTransaction?: boolean
	) {
		return this.txHandler.buildAndSignTransactionMap({
			instructionsMap,
			txVersion: txVersion ?? this.txVersion,
			txParams: txParams ?? this.txParams,
			connection: this.connection,
			preFlightCommitment: this.opts.preflightCommitment,
			fetchMarketLookupTableAccount:
				this.fetchMarketLookupTableAccount.bind(this),
			lookupTables,
			forceVersionedTransaction,
		});
	}
}
