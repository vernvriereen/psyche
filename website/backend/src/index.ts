import { startIndexingChainToDataStores } from './chainTracker.js'

import Fastify, { FastifyRequest } from 'fastify'
import cors from '@fastify/cors'

import {
	ApiGetContributionInfo,
	ApiGetRun,
	ApiGetRuns,
	coordinatorIdl,
	IndexerStatus,
	miningPoolIdl,
	psycheJsonReplacer,
} from 'shared'
import {
	FlatFileCoordinatorDataStore as FlatFileCoordinatorDataStore,
	FlatFileMiningPoolDataStore,
} from './dataStore.js'
import { Connection } from '@solana/web3.js'
import { makeRateLimitedFetch } from './rateLimitedFetch.js'
import { mkdir } from 'fs/promises'

const requiredEnvVars = [
	'COORDINATOR_RPC',
	'MINING_POOL_RPC',
] as const

async function main() {
	const stateDirectory = process.env.STATE_DIRECTORY ?? process.cwd()

	for (const v of requiredEnvVars) {
		if (!process.env[v]) {
			throw new Error(`env var ${v} is not set.`)
		}
	}
	// create working dir so we can write files to it later
	await mkdir(stateDirectory, { recursive: true })

	const coordinatorRpc = new Connection(process.env.COORDINATOR_RPC!, {
		fetch: makeRateLimitedFetch(),
	})
	const coordinatorDataStore = new FlatFileCoordinatorDataStore(
		stateDirectory
	)

	// if the RPCs are the same, use only one to share rate limits.
	const miningPoolRpc =
		process.env.COORDINATOR_RPC === process.env.MINING_POOL_RPC
			? coordinatorRpc
			: new Connection(process.env.MINING_POOL_RPC!, {
					fetch: makeRateLimitedFetch(),
				})

	const miningPoolDataStore = new FlatFileMiningPoolDataStore(stateDirectory)

	const { coordinatorStopped, miningPoolStopped, cancel } =
		await startIndexingChainToDataStores(
			{
				dataStore: coordinatorDataStore,
				connection: coordinatorRpc,
				addressOverride: process.env.COORDINATOR_PROGRAM_ID,
			},
			{
				dataStore: miningPoolDataStore,
				connection: miningPoolRpc,
				addressOverride: process.env.MINING_POOL_PROGRAM_ID,
			}
		)

	const fastify = Fastify({
		logger: true,
	})

	const shutdown = async () => {
		console.log('got shutdown signal, shutting down!')
		cancel()
		await fastify.close()
		await Promise.all([coordinatorStopped, miningPoolStopped])
		process.exit(0)
	}

	let coordinatorError: Error | null = null
	coordinatorStopped.catch((err) => {
		console.error(`[${Date.now()}] coordinator broken: `, err)
		coordinatorError = new Error(err)
	})

	let miningPoolError: Error | null = null
	miningPoolStopped.catch((err) => {
		console.error(`[${Date.now()}] mining pool broken: `, err)
		miningPoolError = new Error(err)
	})

	process.on('SIGTERM', shutdown)

	process.on('SIGINT', shutdown)

	await fastify.register(cors, {
		origin: process.env.CORS_ALLOW_ORIGIN ?? true,
	})

	const initTime = Date.now()

	fastify.get('/contributionInfo', (_req, res) => {
		const data: ApiGetContributionInfo = {
			...miningPoolDataStore.getContributionInfo(),
			miningPoolProgramId: process.env.MINING_POOL_PROGRAM_ID!,
			error: miningPoolError,
		}
		res.header('content-type', 'application/json').send(
			JSON.stringify(data, psycheJsonReplacer)
		)
	})

	fastify.get('/runs', (_req, res) => {
		const runs: ApiGetRuns = {
			runs: coordinatorDataStore.getRunSummaries(),
			error: coordinatorError,
		}

		res.header('content-type', 'application/json').send(
			JSON.stringify(runs, psycheJsonReplacer)
		)
	})

	fastify.get(
		'/run/:runId',
		(req: FastifyRequest<{ Params: { runId?: string } }>, res) => {
			const { runId } = req.params

			const matchingRun = runId
				? coordinatorDataStore.getRunData(runId)
				: null
			const data: ApiGetRun = {
				run: matchingRun,
				error: coordinatorError,
			}
			res.header('content-type', 'application/json').send(
				JSON.stringify(data, psycheJsonReplacer)
			)
		}
	)

	fastify.get('/status', async () => {
		return {
			commit: process.env.GITCOMMIT ?? '???',
			initTime,
			coordinator: {
				status: coordinatorError ? coordinatorError.toString() : 'ok',
				trackedRuns: coordinatorDataStore
					.getRunSummaries()
					.map((r) => ({ id: r.id, status: r.status })),
				chain: {
					chainSlotHeight: await coordinatorRpc.getSlot('confirmed'),
					indexedSlot: coordinatorDataStore.lastProcessedSlot(),
					programId: process.env.COORDINATOR_PROGRAM_ID ?? coordinatorIdl.address,
					networkGenesis: await coordinatorRpc.getGenesisHash(),
				},
			},
			miningPool: {
				status: miningPoolError ? miningPoolError.toString() : 'ok',
				chain: {
					chainSlotHeight: await miningPoolRpc.getSlot('confirmed'),
					indexedSlot: miningPoolDataStore.lastProcessedSlot(),
					programId: process.env.MINING_POOL_PROGRAM_ID ?? miningPoolIdl.address,
					networkGenesis: await miningPoolRpc.getGenesisHash(),
				},
			},
		} satisfies IndexerStatus
	})

	await fastify.listen({ port: 3000 })
}
main()
