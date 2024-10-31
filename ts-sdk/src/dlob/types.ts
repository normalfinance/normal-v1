import { DLOB } from './DLOB';
import { NormalClient } from '../normalClient';

export type DLOBSubscriptionConfig = {
	normalClient: NormalClient;
	dlobSource: DLOBSource;
	slotSource: SlotSource;
	updateFrequency: number;
};

export interface DLOBSubscriberEvents {
	update: (dlob: DLOB) => void;
	error: (e: Error) => void;
}

export interface DLOBSource {
	getDLOB(slot: number): Promise<DLOB>;
}

export interface SlotSource {
	getSlot(): number;
}
