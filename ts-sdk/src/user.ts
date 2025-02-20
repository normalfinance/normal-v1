import { PublicKey } from '@solana/web3.js';
import { EventEmitter } from 'events';
import StrictEventEmitter from 'strict-event-emitter-types';
import { NormalClient } from './normalClient';
import {
	isVariant,
	MarketAccount,
	UserAccount,
	UserStatus,
	UserStatsAccount,
} from './types';
import { ZERO, FUEL_START_TS } from './constants/numericConstants';
import {
	DataAndSlot,
	UserAccountEvents,
	UserAccountSubscriber,
} from './accounts/types';
import { BigNum, BN, divCeil } from '.';
// import { getTokenAmount } from './math/spotBalance';

import { OraclePriceData } from './oracles/types';
import { UserConfig } from './userConfig';
import { PollingUserAccountSubscriber } from './accounts/pollingUserAccountSubscriber';
import { WebSocketUserAccountSubscriber } from './accounts/webSocketUserAccountSubscriber';
// import { getSignedTokenAmount } from './math/spotBalance';

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

	/**
	 * Returns the token amount for a given market. The spot market precision is based on the token mint decimals.
	 * Positive if it is a deposit, negative if it is a borrow.
	 *
	 * @param marketIndex
	 */
	// public getTokenAmount(marketIndex: number): BN {
	// 	const position = this.getPosition(marketIndex);
	// 	if (position === undefined) {
	// 		return ZERO;
	// 	}
	// 	const market = this.normalClient.getMarketAccount(marketIndex);
	// 	return getSignedTokenAmount(
	// 		getTokenAmount(
	// 			position.scaledBalance,
	// 			market,
	// 			position.balanceType
	// 		),
	// 		position.balanceType
	// 	);
	// }

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

	private getOracleDataForMarket(marketIndex: number): OraclePriceData {
		return this.normalClient.getOracleDataForMarket(marketIndex);
	}
}
