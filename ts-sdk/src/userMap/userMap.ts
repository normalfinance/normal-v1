import {
	User,
	NormalClient,
	UserAccount,
	WrappedEvent,
	NewUserRecord,
	LPRecord,
	StateAccount,
	BN,
	UserSubscriptionConfig,
	DataAndSlot,
	OneShotUserAccountSubscriber,
} from '..';

import {
	Commitment,
	Connection,
	PublicKey,
	RpcResponseAndContext,
} from '@solana/web3.js';
import { Buffer } from 'buffer';
import { ZSTDDecoder } from 'zstddec';
import { getNonIdleUserFilter, getUserFilter } from '../memcmp';
import {
	SyncConfig,
	UserAccountFilterCriteria as UserFilterCriteria,
	UserMapConfig,
} from './userMapConfig';
import { WebsocketSubscription } from './WebsocketSubscription';
import { PollingSubscription } from './PollingSubscription';
import { decodeUser } from '../decode/user';

const MAX_USER_ACCOUNT_SIZE_BYTES = 4376;

export interface UserMapInterface {
	subscribe(): Promise<void>;
	unsubscribe(): Promise<void>;
	addPubkey(
		userAccountPublicKey: PublicKey,
		userAccount?: UserAccount,
		slot?: number,
		accountSubscription?: UserSubscriptionConfig
	): Promise<void>;
	has(key: string): boolean;
	get(key: string): User | undefined;
	getWithSlot(key: string): DataAndSlot<User> | undefined;
	mustGet(
		key: string,
		accountSubscription?: UserSubscriptionConfig
	): Promise<User>;
	mustGetWithSlot(
		key: string,
		accountSubscription?: UserSubscriptionConfig
	): Promise<DataAndSlot<User>>;
	getUserAuthority(key: string): PublicKey | undefined;
	// updateWithOrderRecord(record: OrderRecord): Promise<void>;
	values(): IterableIterator<User>;
	valuesWithSlot(): IterableIterator<DataAndSlot<User>>;
	entries(): IterableIterator<[string, User]>;
	entriesWithSlot(): IterableIterator<[string, DataAndSlot<User>]>;
}

export class UserMap implements UserMapInterface {
	private userMap = new Map<string, DataAndSlot<User>>();
	normalClient: NormalClient;
	private connection: Connection;
	private commitment: Commitment;
	private includeIdle: boolean;
	private disableSyncOnTotalAccountsChange: boolean;
	private lastNumberOfSubAccounts: BN;
	private subscription: PollingSubscription | WebsocketSubscription;
	private stateAccountUpdateCallback = async (state: StateAccount) => {
		if (!state.numberOfSubAccounts.eq(this.lastNumberOfSubAccounts)) {
			await this.sync();
			this.lastNumberOfSubAccounts = state.numberOfSubAccounts;
		}
	};
	private decode;
	private mostRecentSlot = 0;
	private syncConfig: SyncConfig;

	private syncPromise?: Promise<void>;
	private syncPromiseResolver: () => void;

	/**
	 * Constructs a new UserMap instance.
	 */
	constructor(config: UserMapConfig) {
		this.normalClient = config.normalClient;
		if (config.connection) {
			this.connection = config.connection;
		} else {
			this.connection = this.normalClient.connection;
		}
		this.commitment =
			config.subscriptionConfig.commitment ?? this.normalClient.opts.commitment;
		this.includeIdle = config.includeIdle ?? false;
		this.disableSyncOnTotalAccountsChange =
			config.disableSyncOnTotalAccountsChange ?? false;

		let decodeFn;
		if (config.fastDecode ?? true) {
			decodeFn = (name, buffer) => decodeUser(buffer);
		} else {
			decodeFn =
				this.normalClient.program.account.user.coder.accounts.decodeUnchecked.bind(
					this.normalClient.program.account.user.coder.accounts
				);
		}
		this.decode = decodeFn;

		if (config.subscriptionConfig.type === 'polling') {
			this.subscription = new PollingSubscription({
				userMap: this,
				frequency: config.subscriptionConfig.frequency,
				skipInitialLoad: config.skipInitialLoad,
			});
		} else {
			this.subscription = new WebsocketSubscription({
				userMap: this,
				commitment: this.commitment,
				resubOpts: {
					resubTimeoutMs: config.subscriptionConfig.resubTimeoutMs,
					logResubMessages: config.subscriptionConfig.logResubMessages,
				},
				skipInitialLoad: config.skipInitialLoad,
				decodeFn,
			});
		}

		this.syncConfig = config.syncConfig ?? {
			type: 'default',
		};
	}
	// updateWithOrderRecord(record: OrderRecord): Promise<void> {
	// 	throw new Error('Method not implemented.');
	// }

	public async subscribe() {
		if (this.size() > 0) {
			return;
		}

		await this.normalClient.subscribe();
		this.lastNumberOfSubAccounts =
			this.normalClient.getStateAccount().numberOfSubAccounts;
		if (!this.disableSyncOnTotalAccountsChange) {
			this.normalClient.eventEmitter.on(
				'stateAccountUpdate',
				this.stateAccountUpdateCallback
			);
		}

		await this.subscription.subscribe();
	}

	public async addPubkey(
		userAccountPublicKey: PublicKey,
		userAccount?: UserAccount,
		slot?: number,
		accountSubscription?: UserSubscriptionConfig
	) {
		const user = new User({
			normalClient: this.normalClient,
			userAccountPublicKey,
			accountSubscription: accountSubscription ?? {
				type: 'custom',
				// OneShotUserAccountSubscriber used here so we don't load up the RPC with AccountSubscribes
				userAccountSubscriber: new OneShotUserAccountSubscriber(
					this.normalClient.program,
					userAccountPublicKey,
					userAccount,
					slot,
					this.commitment
				),
			},
		});
		await user.subscribe(userAccount);
		this.userMap.set(userAccountPublicKey.toString(), {
			data: user,
			slot: slot ?? user.getUserAccountAndSlot()?.slot,
		});
	}

	public has(key: string): boolean {
		return this.userMap.has(key);
	}

	/**
	 * gets the User for a particular userAccountPublicKey, if no User exists, undefined is returned
	 * @param key userAccountPublicKey to get User for
	 * @returns user User | undefined
	 */
	public get(key: string): User | undefined {
		return this.userMap.get(key)?.data;
	}
	public getWithSlot(key: string): DataAndSlot<User> | undefined {
		return this.userMap.get(key);
	}

	/**
	 * gets the User for a particular userAccountPublicKey, if no User exists, new one is created
	 * @param key userAccountPublicKey to get User for
	 * @returns  User
	 */
	public async mustGet(
		key: string,
		accountSubscription?: UserSubscriptionConfig
	): Promise<User> {
		if (!this.has(key)) {
			await this.addPubkey(
				new PublicKey(key),
				undefined,
				undefined,
				accountSubscription
			);
		}
		return this.userMap.get(key).data;
	}
	public async mustGetWithSlot(
		key: string,
		accountSubscription?: UserSubscriptionConfig
	): Promise<DataAndSlot<User>> {
		if (!this.has(key)) {
			await this.addPubkey(
				new PublicKey(key),
				undefined,
				undefined,
				accountSubscription
			);
		}
		return this.userMap.get(key);
	}

	/**
	 * gets the Authority for a particular userAccountPublicKey, if no User exists, undefined is returned
	 * @param key userAccountPublicKey to get User for
	 * @returns authority PublicKey | undefined
	 */
	public getUserAuthority(key: string): PublicKey | undefined {
		const user = this.userMap.get(key);
		if (!user) {
			return undefined;
		}
		return user.data.getUserAccount().authority;
	}

	// public async updateWithOrderRecord(record: OrderRecord) {
	// 	if (!this.has(record.user.toString())) {
	// 		await this.addPubkey(record.user);
	// 	}
	// }

	public async updateWithEventRecord(record: WrappedEvent<any>) {
		// if (record.eventType === 'OrderRecord') {
		// 	const orderRecord = record as OrderRecord;
		// 	await this.updateWithOrderRecord(orderRecord);
		// } else if (record.eventType === 'OrderActionRecord') {
		// 	const actionRecord = record as OrderActionRecord;

		// 	if (actionRecord.taker) {
		// 		await this.mustGet(actionRecord.taker.toString());
		// 	}
		// 	if (actionRecord.maker) {
		// 		await this.mustGet(actionRecord.maker.toString());
		// 	}
		// }
		if (record.eventType === 'NewUserRecord') {
			const newUserRecord = record as NewUserRecord;
			await this.mustGet(newUserRecord.user.toString());
		} else if (record.eventType === 'LPRecord') {
			const lpRecord = record as LPRecord;
			await this.mustGet(lpRecord.user.toString());
		}
	}

	public *values(): IterableIterator<User> {
		for (const dataAndSlot of this.userMap.values()) {
			yield dataAndSlot.data;
		}
	}
	public valuesWithSlot(): IterableIterator<DataAndSlot<User>> {
		return this.userMap.values();
	}

	public *entries(): IterableIterator<[string, User]> {
		for (const [key, dataAndSlot] of this.userMap.entries()) {
			yield [key, dataAndSlot.data];
		}
	}
	public entriesWithSlot(): IterableIterator<[string, DataAndSlot<User>]> {
		return this.userMap.entries();
	}

	public size(): number {
		return this.userMap.size;
	}

	/**
	 * Returns a unique list of authorities for all users in the UserMap that meet the filter criteria
	 * @param filterCriteria: Users must meet these criteria to be included
	 * @returns
	 */
	public getUniqueAuthorities(
		filterCriteria?: UserFilterCriteria
	): PublicKey[] {
		const usersMeetingCriteria = Array.from(this.values()).filter((user) => {
			let pass = true;
			if (filterCriteria && filterCriteria.hasOpenOrders) {
				pass = pass && user.getUserAccount().hasOpenOrder;
			}
			return pass;
		});
		const userAuths = new Set(
			usersMeetingCriteria.map((user) =>
				user.getUserAccount().authority.toBase58()
			)
		);
		const userAuthKeys = Array.from(userAuths).map(
			(userAuth) => new PublicKey(userAuth)
		);
		return userAuthKeys;
	}

	public async sync() {
		if (this.syncConfig.type === 'default') {
			return this.defaultSync();
		} else {
			return this.paginatedSync();
		}
	}

	private async defaultSync() {
		if (this.syncPromise) {
			return this.syncPromise;
		}
		this.syncPromise = new Promise((resolver) => {
			this.syncPromiseResolver = resolver;
		});

		try {
			const filters = [getUserFilter()];
			if (!this.includeIdle) {
				filters.push(getNonIdleUserFilter());
			}

			const rpcRequestArgs = [
				this.normalClient.program.programId.toBase58(),
				{
					commitment: this.commitment,
					filters,
					encoding: 'base64+zstd',
					withContext: true,
				},
			];

			// @ts-ignore
			const rpcJSONResponse: any = await this.connection._rpcRequest(
				'getProgramAccounts',
				rpcRequestArgs
			);
			const rpcResponseAndContext: RpcResponseAndContext<
				Array<{ pubkey: PublicKey; account: { data: [string, string] } }>
			> = rpcJSONResponse.result;
			const slot = rpcResponseAndContext.context.slot;

			this.updateLatestSlot(slot);

			const programAccountBufferMap = new Map<string, Buffer>();
			const decodingPromises = rpcResponseAndContext.value.map(
				async (programAccount) => {
					const compressedUserData = Buffer.from(
						programAccount.account.data[0],
						'base64'
					);
					const decoder = new ZSTDDecoder();
					await decoder.init();
					const userBuffer = decoder.decode(
						compressedUserData,
						MAX_USER_ACCOUNT_SIZE_BYTES
					);
					programAccountBufferMap.set(
						programAccount.pubkey.toString(),
						Buffer.from(userBuffer)
					);
				}
			);

			await Promise.all(decodingPromises);

			const promises = Array.from(programAccountBufferMap.entries()).map(
				([key, buffer]) =>
					(async () => {
						const currAccountWithSlot = this.getWithSlot(key);
						if (currAccountWithSlot) {
							if (slot >= currAccountWithSlot.slot) {
								const userAccount = this.decode('User', buffer);
								this.updateUserAccount(key, userAccount, slot);
							}
						} else {
							const userAccount = this.decode('User', buffer);
							await this.addPubkey(new PublicKey(key), userAccount, slot);
						}
					})()
			);

			await Promise.all(promises);

			for (const [key] of this.entries()) {
				if (!programAccountBufferMap.has(key)) {
					const user = this.get(key);
					if (user) {
						await user.unsubscribe();
						this.userMap.delete(key);
					}
				}
			}
		} catch (err) {
			const e = err as Error;
			console.error(`Error in UserMap.sync(): ${e.message} ${e.stack ?? ''}`);
		} finally {
			this.syncPromiseResolver();
			this.syncPromise = undefined;
		}
	}

	private async paginatedSync() {
		if (this.syncPromise) {
			return this.syncPromise;
		}

		this.syncPromise = new Promise<void>((resolve) => {
			this.syncPromiseResolver = resolve;
		});

		try {
			const accountsPrefetch = await this.connection.getProgramAccounts(
				this.normalClient.program.programId,
				{
					dataSlice: { offset: 0, length: 0 },
					filters: [
						getUserFilter(),
						...(!this.includeIdle ? [getNonIdleUserFilter()] : []),
					],
				}
			);
			const accountPublicKeys = accountsPrefetch.map(
				(account) => account.pubkey
			);

			const limitConcurrency = async (tasks, limit) => {
				const executing = [];
				const results = [];

				for (let i = 0; i < tasks.length; i++) {
					const executor = Promise.resolve().then(tasks[i]);
					results.push(executor);

					if (executing.length < limit) {
						executing.push(executor);
						executor.finally(() => {
							const index = executing.indexOf(executor);
							if (index > -1) {
								executing.splice(index, 1);
							}
						});
					} else {
						await Promise.race(executing);
					}
				}

				return Promise.all(results);
			};

			const programAccountBufferMap = new Map<string, Buffer>();

			// @ts-ignore
			const chunkSize = this.syncConfig.chunkSize ?? 100;
			const tasks = [];
			for (let i = 0; i < accountPublicKeys.length; i += chunkSize) {
				const chunk = accountPublicKeys.slice(i, i + chunkSize);
				tasks.push(async () => {
					const accountInfos =
						await this.connection.getMultipleAccountsInfoAndContext(chunk, {
							commitment: this.commitment,
						});

					const accountInfosSlot = accountInfos.context.slot;

					for (let j = 0; j < accountInfos.value.length; j += 1) {
						const accountInfo = accountInfos.value[j];
						if (accountInfo === null) continue;

						const publicKeyString = chunk[j].toString();
						const buffer = Buffer.from(accountInfo.data);
						programAccountBufferMap.set(publicKeyString, buffer);

						const decodedUser = this.decode('User', buffer);

						const currAccountWithSlot = this.getWithSlot(publicKeyString);
						if (
							currAccountWithSlot &&
							currAccountWithSlot.slot <= accountInfosSlot
						) {
							this.updateUserAccount(
								publicKeyString,
								decodedUser,
								accountInfosSlot
							);
						} else {
							await this.addPubkey(
								new PublicKey(publicKeyString),
								decodedUser,
								accountInfosSlot
							);
						}
					}
				});
			}

			// @ts-ignore
			const concurrencyLimit = this.syncConfig.concurrencyLimit ?? 10;
			await limitConcurrency(tasks, concurrencyLimit);

			for (const [key] of this.entries()) {
				if (!programAccountBufferMap.has(key)) {
					const user = this.get(key);
					if (user) {
						await user.unsubscribe();
						this.userMap.delete(key);
					}
				}
			}
		} catch (err) {
			console.error(`Error in UserMap.sync():`, err);
		} finally {
			if (this.syncPromiseResolver) {
				this.syncPromiseResolver();
			}
			this.syncPromise = undefined;
		}
	}

	public async unsubscribe() {
		await this.subscription.unsubscribe();

		for (const [key, user] of this.entries()) {
			await user.unsubscribe();
			this.userMap.delete(key);
		}

		if (this.lastNumberOfSubAccounts) {
			if (!this.disableSyncOnTotalAccountsChange) {
				this.normalClient.eventEmitter.removeListener(
					'stateAccountUpdate',
					this.stateAccountUpdateCallback
				);
			}

			this.lastNumberOfSubAccounts = undefined;
		}
	}

	public async updateUserAccount(
		key: string,
		userAccount: UserAccount,
		slot: number
	) {
		const userWithSlot = this.getWithSlot(key);
		this.updateLatestSlot(slot);
		if (userWithSlot) {
			if (slot >= userWithSlot.slot) {
				userWithSlot.data.accountSubscriber.updateData(userAccount, slot);
				this.userMap.set(key, {
					data: userWithSlot.data,
					slot,
				});
			}
		} else {
			this.addPubkey(new PublicKey(key), userAccount, slot);
		}
	}

	updateLatestSlot(slot: number): void {
		this.mostRecentSlot = Math.max(slot, this.mostRecentSlot);
	}

	public getSlot(): number {
		return this.mostRecentSlot;
	}
}
