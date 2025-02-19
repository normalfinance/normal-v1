import { isVariant, OracleSource } from '../types';
import { Connection } from '@solana/web3.js';
import { OracleClient } from '../oracles/types';
import { PythClient } from '../oracles/pythClient';
import { QuoteAssetOracleClient } from '../oracles/quoteAssetOracleClient';
import { BN } from '@coral-xyz/anchor';
import { PythPullClient } from '../oracles/pythPullClient';

export function getOracleClient(
	oracleSource: OracleSource,
	connection: Connection
): OracleClient {
	if (isVariant(oracleSource, 'pyth')) {
		return new PythClient(connection);
	}

	if (isVariant(oracleSource, 'pythPull')) {
		return new PythPullClient(connection);
	}

	if (isVariant(oracleSource, 'pyth1K')) {
		return new PythClient(connection, new BN(1000));
	}

	if (isVariant(oracleSource, 'pyth1KPull')) {
		return new PythPullClient(connection, new BN(1000));
	}

	if (isVariant(oracleSource, 'pyth1M')) {
		return new PythClient(connection, new BN(1000000));
	}

	if (isVariant(oracleSource, 'pyth1MPull')) {
		return new PythPullClient(connection, new BN(1000000));
	}

	if (isVariant(oracleSource, 'pythStableCoin')) {
		return new PythClient(connection, undefined, true);
	}

	if (isVariant(oracleSource, 'pythStableCoinPull')) {
		return new PythPullClient(connection, undefined, true);
	}

	if (isVariant(oracleSource, 'quoteAsset')) {
		return new QuoteAssetOracleClient();
	}

	throw new Error(`Unknown oracle source ${oracleSource}`);
}
