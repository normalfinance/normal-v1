import fetch from 'node-fetch';
import { HeliusPriorityLevel } from './heliusPriorityFeeMethod';

export type NormalMarketInfo = {
	marketType: string;
	marketIndex: number;
};

export type NormalPriorityFeeLevels = {
	[key in HeliusPriorityLevel]: number;
} & {
	marketType: 'synth';
	marketIndex: number;
};

export type NormalPriorityFeeResponse = NormalPriorityFeeLevels[];

export async function fetchNormalPriorityFee(
	url: string,
	marketTypes: string[],
	marketIndexes: number[]
): Promise<NormalPriorityFeeResponse> {
	try {
		const response = await fetch(
			`${url}/batchPriorityFees?marketType=${marketTypes.join(
				','
			)}&marketIndex=${marketIndexes.join(',')}`
		);
		if (!response.ok) {
			throw new Error(`HTTP error! status: ${response.status}`);
		}
		return await response.json();
	} catch (err) {
		if (err instanceof Error) {
			console.error('Error fetching priority fees:', err.message);
		} else {
			console.error('Unknown error fetching priority fees:', err);
		}
	}

	return [];
}
