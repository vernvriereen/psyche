import { PsycheCoordinator } from 'psyche-deserialize-zerocopy-wasm'
import { RunSummary, RunData, ContributionInfo, ChainTimestamp } from 'shared'
import { PsycheMiningPoolAccount, WitnessMetadata } from './idlTypes.js'
import { PublicKey } from '@solana/web3.js'

export interface IndexedSignature {
	signature: string
	slot: number
}

export interface LastUpdateInfo {
	time: Date
	highestSignature?: IndexedSignature
}

export interface ChainDataStore {
	lastUpdate(): LastUpdateInfo
	sync(lastUpdateInfo: LastUpdateInfo): Promise<void>
}

export interface CoordinatorDataStore extends ChainDataStore {
	createRun(
		pubkey: string,
		runId: string,
		timestamp: ChainTimestamp,
		// it's possible that we never get a state, if the run was created then destroyed while we're offline.
		newState?: PsycheCoordinator
	): void
	updateRun(
		pubkey: string,
		newState: PsycheCoordinator,
		timestamp: ChainTimestamp
	): void
	setRunPaused(pubkey: string, paused: boolean, timestamp: ChainTimestamp): void
	witnessRun(
		pubkey: string,
		witness: WitnessMetadata,
		timestamp: ChainTimestamp
	): void
	destroyRun(pubkey: string, timestamp: ChainTimestamp): void

	getRunSummaries(): RunSummary[]
	getRunData(publickey: PublicKey, index?: number): RunData | null
	getRunDataById(runId: string, index?: number): RunData | null
}

export interface MiningPoolDataStore extends ChainDataStore {
	setFundingData(data: PsycheMiningPoolAccount): void
	setUserAmount(address: string, amount: bigint): void
	setCollateralInfo(mintAddress: string, decimals: number): void
	hasCollateralInfo(): boolean
	getContributionInfo(): Omit<ContributionInfo, 'miningPoolProgramId'>
}
