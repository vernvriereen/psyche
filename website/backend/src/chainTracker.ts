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
import { startWatchMiningPoolChainLoop } from './miningPoolChainLoop.js'

export function startIndexingChainToDataStores(
	coordinator: {
		connection: Connection
		dataStore: CoordinatorDataStore
		addressOverride?: string
	},
	miningPool: {
		connection: Connection
		dataStore: MiningPoolDataStore
		addressOverride?: string
	}
): {
	cancel: () => void
	coordinatorStopped: Promise<void>
	miningPoolStopped: Promise<void>
} {
	const cancelled = { cancelled: false }

	console.log('Initializing watch chain loop for coordinator & mining pool:')
	console.log(`Coordinator address: ${coordinator.addressOverride}`)
	console.log(`Coordinator RPC: ${coordinator.connection.rpcEndpoint}`)
	console.log(`MiningPool address: ${miningPool.addressOverride}`)
	console.log(`MiningPool RPC: ${miningPool.connection.rpcEndpoint}`)

	let coordinatorRes!: () => void
	let coordinatorRej!: (reason?: any) => void
	const coordinatorStopped = new Promise<void>((res, rej) => {
		coordinatorRes = res
		coordinatorRej = rej
	})

	try {
		const coordinatorProgram = new Program<PsycheSolanaCoordinator>(
			coordinator.addressOverride
				? { ...coordinatorIdl, address: coordinator.addressOverride }
				: (coordinatorIdl as any),
			coordinator
		)
		startWatchCoordinatorChainLoop(
			coordinator.dataStore,
			coordinatorProgram,
			cancelled
		)
			.catch(coordinatorRej)
			.then(coordinatorRes)
	} catch (err) {
		coordinatorRej(err)
	}

	let miningPoolRes!: () => void
	let miningPoolRej!: (reason?: any) => void
	const miningPoolStopped = new Promise<void>((res, rej) => {
		miningPoolRes = res
		miningPoolRej = rej
	})
	try {
		const miningPoolProgram = new Program<PsycheSolanaMiningPool>(
			miningPool.addressOverride
				? { ...miningPoolIdl, address: miningPool.addressOverride }
				: (miningPoolIdl as any),
			miningPool
		)
		startWatchMiningPoolChainLoop(
			miningPool.dataStore,
			miningPoolProgram,
			cancelled
		)
			.catch(miningPoolRej)
			.then(miningPoolRes)
	} catch (err) {
		miningPoolRej(err)
	}

	const cancel = () => {
		cancelled.cancelled = true
	}

	return { coordinatorStopped, miningPoolStopped, cancel }
}
