import { Program } from '@coral-xyz/anchor'
import { Connection } from '@solana/web3.js'
import {
	coordinatorIdl,
	miningPoolIdl,
	PsycheSolanaCoordinator,
	PsycheSolanaMiningPool,
} from 'shared'
import { CoordinatorDataStore, MiningPoolDataStore } from './dataStore.js'
import { startWatchCoordinatorChainLoop } from './coordinatorChainLoop.js'
import { mkdirSync } from 'fs'
import { FlatFileCoordinatorDataStore } from './dataStores/flatFileCoordinator.js'
import { FlatFileMiningPoolDataStore } from './dataStores/flatFileMiningPool.js'
import { startWatchMiningPoolChainLoop } from './miningPoolChainLoop.js'

export function startIndexingChainToDataStores(
	coordinator: {
		connection: Connection
		addressOverride?: string
	},
	miningPool: {
		connection: Connection
		addressOverride?: string
	}
): {
	cancel: () => void
	coordinator: { stopped: Promise<void>; dataStore: CoordinatorDataStore }
	miningPool: { stopped: Promise<void>; dataStore: MiningPoolDataStore }
} {
	const stateDirectory = process.env.STATE_DIRECTORY ?? process.cwd()

	// create working dir so we can write files to it later
	mkdirSync(stateDirectory, { recursive: true })

	const cancelled = { cancelled: false }

	let coordinatorRes!: () => void
	let coordinatorRej!: (reason?: any) => void
	const coordinatorStopped = new Promise<void>((res, rej) => {
		coordinatorRes = res
		coordinatorRej = rej
	})

	const coordinatorProgram = new Program<PsycheSolanaCoordinator>(
		coordinator.addressOverride
			? { ...coordinatorIdl, address: coordinator.addressOverride }
			: (coordinatorIdl as any),
		coordinator
	)

	const coordinatorDataStore = new FlatFileCoordinatorDataStore(
		stateDirectory,
		coordinatorProgram.programId
	)
	startWatchCoordinatorChainLoop(
		coordinatorDataStore,
		coordinatorProgram,
		cancelled
	)
		.catch(coordinatorRej)
		.then(coordinatorRes)

	let miningPoolRes!: () => void
	let miningPoolRej!: (reason?: any) => void
	const miningPoolStopped = new Promise<void>((res, rej) => {
		miningPoolRes = res
		miningPoolRej = rej
	})
	const miningPoolProgram = new Program<PsycheSolanaMiningPool>(
		miningPool.addressOverride
			? { ...miningPoolIdl, address: miningPool.addressOverride }
			: (miningPoolIdl as any),
		miningPool
	)
	const miningPoolDataStore = new FlatFileMiningPoolDataStore(
		stateDirectory,
		miningPoolProgram.programId
	)

	console.log('Initializing watch chain loop for coordinator & mining pool:')
	console.log(`Coordinator ProgramID: ${coordinatorProgram.programId}`)
	console.log(`Coordinator RPC: ${coordinator.connection.rpcEndpoint}`)
	console.log(`MiningPool ProgramID: ${miningPoolProgram.programId}`)
	console.log(`MiningPool RPC: ${miningPool.connection.rpcEndpoint}`)

	startWatchMiningPoolChainLoop(
		miningPoolDataStore,
		miningPoolProgram,
		cancelled
	)
		.catch(miningPoolRej)
		.then(miningPoolRes)

	return {
		coordinator: {
			stopped: coordinatorStopped,
			dataStore: coordinatorDataStore,
		},
		miningPool: {
			stopped: miningPoolStopped,
			dataStore: miningPoolDataStore,
		},
		cancel: () => {
			cancelled.cancelled = true
		},
	}
}
