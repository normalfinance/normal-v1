import {
	PublicKey,
	SystemProgram,
	SYSVAR_RENT_PUBKEY,
	TransactionInstruction,
	TransactionSignature,
} from '@solana/web3.js';
import {
	FeeStructure,
	OracleGuardRails,
	OracleSource,
	ExchangeStatus,
} from './types';
import { DEFAULT_MARKET_NAME, encodeName } from './userName';
import { BN } from '@coral-xyz/anchor';
import * as anchor from '@coral-xyz/anchor';
import {
	getNormalStateAccountPublicKeyAndNonce,
	getMarketPublicKey,
	getInsuranceFundVaultPublicKey,
	getProtocolIfSharesTransferConfigPublicKey,
	getPythPullOraclePublicKey,
	getUserStatsAccountPublicKey,
	getInsuranceFundPublicKey,
} from './addresses/pda';
import { squareRootBN } from './math/utils';
import { TOKEN_PROGRAM_ID } from '@solana/spl-token';
import { NormalClient } from './normalClient';
import {
	PEG_PRECISION,
	QUOTE_SPOT_MARKET_INDEX,
	ZERO,
	ONE,
	BASE_PRECISION,
	PRICE_PRECISION,
} from './constants/numericConstants';
import { NORMAL_ORACLE_RECEIVER_ID } from './config';
import { getFeedIdUint8Array } from './util/pythPullOracleUtils';

export class AdminClient extends NormalClient {
	public async initialize(
		usdcMint: PublicKey,
		_adminControlsPrices: boolean
	): Promise<[TransactionSignature]> {
		const stateAccountRPCResponse = await this.connection.getParsedAccountInfo(
			await this.getStatePublicKey()
		);
		if (stateAccountRPCResponse.value !== null) {
			throw new Error('Clearing house already initialized');
		}

		const [normalStatePublicKey] = await getNormalStateAccountPublicKeyAndNonce(
			this.program.programId
		);

		const initializeIx = await this.program.instruction.initialize({
			accounts: {
				admin: this.isSubscribed
					? this.getStateAccount().admin
					: this.wallet.publicKey,
				state: normalStatePublicKey,
				quoteAssetMint: usdcMint,
				rent: SYSVAR_RENT_PUBKEY,
				normalSigner: this.getSignerPublicKey(),
				systemProgram: anchor.web3.SystemProgram.programId,
				tokenProgram: TOKEN_PROGRAM_ID,
			},
		});

		const tx = await this.buildTransaction(initializeIx);

		const { txSig } = await super.sendTransaction(tx, [], this.opts);

		return [txSig];
	}

	public async updateInitialPctToLiquidate(
		initialPctToLiquidate: number
	): Promise<TransactionSignature> {
		const updateInitialPctToLiquidateIx =
			await this.getUpdateInitialPctToLiquidateIx(initialPctToLiquidate);

		const tx = await this.buildTransaction(updateInitialPctToLiquidateIx);

		const { txSig } = await this.sendTransaction(tx, [], this.opts);

		return txSig;
	}

	public async getUpdateInitialPctToLiquidateIx(
		initialPctToLiquidate: number
	): Promise<TransactionInstruction> {
		return await this.program.instruction.updateInitialPctToLiquidate(
			initialPctToLiquidate,
			{
				accounts: {
					admin: this.isSubscribed
						? this.getStateAccount().admin
						: this.wallet.publicKey,
					state: await this.getStatePublicKey(),
				},
			}
		);
	}

	public async updateLiquidationDuration(
		liquidationDuration: number
	): Promise<TransactionSignature> {
		const updateLiquidationDurationIx =
			await this.getUpdateLiquidationDurationIx(liquidationDuration);

		const tx = await this.buildTransaction(updateLiquidationDurationIx);

		const { txSig } = await this.sendTransaction(tx, [], this.opts);

		return txSig;
	}

	public async getUpdateLiquidationDurationIx(
		liquidationDuration: number
	): Promise<TransactionInstruction> {
		return await this.program.instruction.updateLiquidationDuration(
			liquidationDuration,
			{
				accounts: {
					admin: this.isSubscribed
						? this.getStateAccount().admin
						: this.wallet.publicKey,
					state: await this.getStatePublicKey(),
				},
			}
		);
	}

	public async updateLiquidationMarginBufferRatio(
		updateLiquidationMarginBufferRatio: number
	): Promise<TransactionSignature> {
		const updateLiquidationMarginBufferRatioIx =
			await this.getUpdateLiquidationMarginBufferRatioIx(
				updateLiquidationMarginBufferRatio
			);

		const tx = await this.buildTransaction(
			updateLiquidationMarginBufferRatioIx
		);

		const { txSig } = await this.sendTransaction(tx, [], this.opts);

		return txSig;
	}

	public async getUpdateLiquidationMarginBufferRatioIx(
		updateLiquidationMarginBufferRatio: number
	): Promise<TransactionInstruction> {
		return await this.program.instruction.updateLiquidationMarginBufferRatio(
			updateLiquidationMarginBufferRatio,
			{
				accounts: {
					admin: this.isSubscribed
						? this.getStateAccount().admin
						: this.wallet.publicKey,
					state: await this.getStatePublicKey(),
				},
			}
		);
	}

	public async updateOracleGuardRails(
		oracleGuardRails: OracleGuardRails
	): Promise<TransactionSignature> {
		const updateOracleGuardRailsIx = await this.getUpdateOracleGuardRailsIx(
			oracleGuardRails
		);

		const tx = await this.buildTransaction(updateOracleGuardRailsIx);

		const { txSig } = await this.sendTransaction(tx, [], this.opts);

		return txSig;
	}

	public async getUpdateOracleGuardRailsIx(
		oracleGuardRails: OracleGuardRails
	): Promise<TransactionInstruction> {
		return await this.program.instruction.updateOracleGuardRails(
			oracleGuardRails,
			{
				accounts: {
					admin: this.isSubscribed
						? this.getStateAccount().admin
						: this.wallet.publicKey,
					state: await this.getStatePublicKey(),
				},
			}
		);
	}

	public async updateStateSettlementDuration(
		settlementDuration: number
	): Promise<TransactionSignature> {
		const updateStateSettlementDurationIx =
			await this.getUpdateStateSettlementDurationIx(settlementDuration);

		const tx = await this.buildTransaction(updateStateSettlementDurationIx);

		const { txSig } = await this.sendTransaction(tx, [], this.opts);

		return txSig;
	}

	public async getUpdateStateSettlementDurationIx(
		settlementDuration: number
	): Promise<TransactionInstruction> {
		return await this.program.instruction.updateStateSettlementDuration(
			settlementDuration,
			{
				accounts: {
					admin: this.isSubscribed
						? this.getStateAccount().admin
						: this.wallet.publicKey,
					state: await this.getStatePublicKey(),
				},
			}
		);
	}

	public async updateStateMaxNumberOfSubAccounts(
		maxNumberOfSubAccounts: number
	): Promise<TransactionSignature> {
		const updateStateMaxNumberOfSubAccountsIx =
			await this.getUpdateStateMaxNumberOfSubAccountsIx(maxNumberOfSubAccounts);

		const tx = await this.buildTransaction(updateStateMaxNumberOfSubAccountsIx);

		const { txSig } = await this.sendTransaction(tx, [], this.opts);

		return txSig;
	}

	public async getUpdateStateMaxNumberOfSubAccountsIx(
		maxNumberOfSubAccounts: number
	): Promise<TransactionInstruction> {
		return await this.program.instruction.updateStateMaxNumberOfSubAccounts(
			maxNumberOfSubAccounts,
			{
				accounts: {
					admin: this.isSubscribed
						? this.getStateAccount().admin
						: this.wallet.publicKey,
					state: await this.getStatePublicKey(),
				},
			}
		);
	}

	public async updateStateMaxInitializeUserFee(
		maxInitializeUserFee: number
	): Promise<TransactionSignature> {
		const updateStateMaxInitializeUserFeeIx =
			await this.getUpdateStateMaxInitializeUserFeeIx(maxInitializeUserFee);

		const tx = await this.buildTransaction(updateStateMaxInitializeUserFeeIx);

		const { txSig } = await this.sendTransaction(tx, [], this.opts);

		return txSig;
	}

	public async getUpdateStateMaxInitializeUserFeeIx(
		maxInitializeUserFee: number
	): Promise<TransactionInstruction> {
		return await this.program.instruction.updateStateMaxInitializeUserFee(
			maxInitializeUserFee,
			{
				accounts: {
					admin: this.isSubscribed
						? this.getStateAccount().admin
						: this.wallet.publicKey,
					state: await this.getStatePublicKey(),
				},
			}
		);
	}

	public async updateInsuranceFundUnstakingPeriod(
		insuranceWithdrawEscrowPeriod: BN
	): Promise<TransactionSignature> {
		const updateInsuranceFundUnstakingPeriodIx =
			await this.getUpdateInsuranceFundUnstakingPeriodIx(
				insuranceWithdrawEscrowPeriod
			);

		const tx = await this.buildTransaction(
			updateInsuranceFundUnstakingPeriodIx
		);

		const { txSig } = await this.sendTransaction(tx, [], this.opts);

		return txSig;
	}

	public async getUpdateInsuranceFundUnstakingPeriodIx(
		insuranceWithdrawEscrowPeriod: BN
	): Promise<TransactionInstruction> {
		return await this.program.instruction.updateInsuranceFundUnstakingPeriod(
			insuranceWithdrawEscrowPeriod,
			{
				accounts: {
					admin: this.isSubscribed
						? this.getStateAccount().admin
						: this.wallet.publicKey,
					state: await this.getStatePublicKey(),
					insuranceFund: await getInsuranceFundPublicKey(
						this.program.programId
					),
				},
			}
		);
	}

	public async updateIfPausedOperations(
		pausedOperations: number
	): Promise<TransactionSignature> {
		const updateIfStakingDisabledIx = await this.getUpdateIfPausedOperationsIx(
			pausedOperations
		);

		const tx = await this.buildTransaction(updateIfStakingDisabledIx);

		const { txSig } = await this.sendTransaction(tx, [], this.opts);

		return txSig;
	}

	public async getUpdateIfPausedOperationsIx(
		pausedOperations: number
	): Promise<TransactionInstruction> {
		return await this.program.instruction.updateIfPausedOperations(
			pausedOperations,
			{
				accounts: {
					admin: this.isSubscribed
						? this.getStateAccount().admin
						: this.wallet.publicKey,
					state: await this.getStatePublicKey(),
				},
			}
		);
	}

	public async updateExchangeStatus(
		exchangeStatus: ExchangeStatus
	): Promise<TransactionSignature> {
		const updateExchangeStatusIx = await this.getUpdateExchangeStatusIx(
			exchangeStatus
		);

		const tx = await this.buildTransaction(updateExchangeStatusIx);

		const { txSig } = await this.sendTransaction(tx, [], this.opts);

		return txSig;
	}

	public async getUpdateExchangeStatusIx(
		exchangeStatus: ExchangeStatus
	): Promise<TransactionInstruction> {
		return await this.program.instruction.updateExchangeStatus(exchangeStatus, {
			accounts: {
				admin: this.isSubscribed
					? this.getStateAccount().admin
					: this.wallet.publicKey,
				state: await this.getStatePublicKey(),
			},
		});
	}

	public async initializeProtocolIfSharesTransferConfig(): Promise<TransactionSignature> {
		const initializeProtocolIfSharesTransferConfigIx =
			await this.getInitializeProtocolIfSharesTransferConfigIx();

		const tx = await this.buildTransaction(
			initializeProtocolIfSharesTransferConfigIx
		);

		const { txSig } = await this.sendTransaction(tx, [], this.opts);

		return txSig;
	}

	public async getInitializeProtocolIfSharesTransferConfigIx(): Promise<TransactionInstruction> {
		return await this.program.instruction.initializeProtocolIfSharesTransferConfig(
			{
				accounts: {
					admin: this.isSubscribed
						? this.getStateAccount().admin
						: this.wallet.publicKey,
					state: await this.getStatePublicKey(),
					rent: SYSVAR_RENT_PUBKEY,
					systemProgram: anchor.web3.SystemProgram.programId,
					protocolIfSharesTransferConfig:
						getProtocolIfSharesTransferConfigPublicKey(this.program.programId),
				},
			}
		);
	}

	public async updateProtocolIfSharesTransferConfig(
		whitelistedSigners?: PublicKey[],
		maxTransferPerEpoch?: BN
	): Promise<TransactionSignature> {
		const updateProtocolIfSharesTransferConfigIx =
			await this.getUpdateProtocolIfSharesTransferConfigIx(
				whitelistedSigners,
				maxTransferPerEpoch
			);

		const tx = await this.buildTransaction(
			updateProtocolIfSharesTransferConfigIx
		);

		const { txSig } = await this.sendTransaction(tx, [], this.opts);

		return txSig;
	}

	public async getUpdateProtocolIfSharesTransferConfigIx(
		whitelistedSigners?: PublicKey[],
		maxTransferPerEpoch?: BN
	): Promise<TransactionInstruction> {
		return await this.program.instruction.updateProtocolIfSharesTransferConfig(
			whitelistedSigners || null,
			maxTransferPerEpoch,
			{
				accounts: {
					admin: this.isSubscribed
						? this.getStateAccount().admin
						: this.wallet.publicKey,
					state: await this.getStatePublicKey(),
					protocolIfSharesTransferConfig:
						getProtocolIfSharesTransferConfigPublicKey(this.program.programId),
				},
			}
		);
	}

	public async initializePythPullOracle(
		feedId: string
	): Promise<TransactionSignature> {
		const initializePythPullOracleIx = await this.getInitializePythPullOracleIx(
			feedId
		);
		const tx = await this.buildTransaction(initializePythPullOracleIx);
		const { txSig } = await this.sendTransaction(tx, [], this.opts);

		return txSig;
	}

	public async getInitializePythPullOracleIx(
		feedId: string
	): Promise<TransactionInstruction> {
		const feedIdBuffer = getFeedIdUint8Array(feedId);
		return await this.program.instruction.initializePythPullOracle(
			feedIdBuffer,
			{
				accounts: {
					admin: this.isSubscribed
						? this.getStateAccount().admin
						: this.wallet.publicKey,
					state: await this.getStatePublicKey(),
					systemProgram: SystemProgram.programId,
					priceFeed: getPythPullOraclePublicKey(
						this.program.programId,
						feedIdBuffer
					),
					pythSolanaReceiver: NORMAL_ORACLE_RECEIVER_ID,
				},
			}
		);
	}
}
