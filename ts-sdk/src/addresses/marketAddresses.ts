import { PublicKey } from '@solana/web3.js';
import { getSynthMarketPublicKey } from './pda';

const CACHE = new Map<string, PublicKey>();
export async function getMarketAddress(
	programId: PublicKey,
	marketIndex: number
): Promise<PublicKey> {
	const cacheKey = `${programId.toString()}-${marketIndex.toString()}`;
	if (CACHE.has(cacheKey)) {
		return CACHE.get(cacheKey);
	}

	const publicKey = await getSynthMarketPublicKey(programId, marketIndex);
	CACHE.set(cacheKey, publicKey);
	return publicKey;
}
