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
	RunData,
} from 'shared'
import { Connection } from '@solana/web3.js'
import { makeRateLimitedFetch } from './rateLimit.js'
import { PassThrough } from 'node:stream'

const requiredEnvVars = ['COORDINATOR_RPC', 'MINING_POOL_RPC'] as const

async function main() {
	for (const v of requiredEnvVars) {
		if (!process.env[v]) {
			throw new Error(`env var ${v} is not set.`)
		}
	}
	const coordinatorRpc = new Connection(process.env.COORDINATOR_RPC!, {
		fetch: makeRateLimitedFetch(),
	})

	// if the RPCs are the same, use only one to share rate limits.
	const miningPoolRpc =
		process.env.COORDINATOR_RPC === process.env.MINING_POOL_RPC
			? coordinatorRpc
			: new Connection(process.env.MINING_POOL_RPC!, {
					fetch: makeRateLimitedFetch(),
				})

	const { coordinator, miningPool, cancel } =
		await startIndexingChainToDataStores(
			{
				connection: coordinatorRpc,
				addressOverride: process.env.COORDINATOR_PROGRAM_ID,
				websocketRpcUrl: process.env.COORDINATOR_WS_RPC,
			},
			{
				connection: miningPoolRpc,
				addressOverride: process.env.MINING_POOL_PROGRAM_ID,
				websocketRpcUrl: process.env.MINING_POOL_WS_RPC,
			}
		)

	type RunId = string
	const liveRunListeners: Map<
		RunId,
		Set<(runData: RunData) => void>
	> = new Map()
	coordinator.dataStore.eventEmitter.addListener('update', (runId) => {
		const listeners = liveRunListeners.get(runId)
		if (listeners) {
			const runData = coordinator.dataStore.getRunDataById(runId)
			if (!runData) {
				console.warn(
					`Tried to emit updates for run ${runId} but it has no data!`
				)
				return
			}
			for (const listener of listeners) {
				try {
					listener(runData)
				} catch (err) {
					console.error(
						`Failed to send run data for run ${runId} to subscribed client...`
					)
				}
			}
		}
	})

	const fastify = Fastify({
		logger: true,
	})

	const shutdown = async () => {
		console.log('got shutdown signal, shutting down!')
		cancel()
		await fastify.close()
		await Promise.all([coordinator.stopped, miningPool.stopped])
		process.exit(0)
	}

	let coordinatorError: Error | null = null
	coordinator.stopped.catch((err) => {
		console.error(`[${Date.now()}] coordinator broken: `, err)
		coordinatorError = new Error(err)
	})

	let miningPoolError: Error | null = null
	miningPool.stopped.catch((err) => {
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
			...miningPool.dataStore.getContributionInfo(),
			miningPoolProgramId: process.env.MINING_POOL_PROGRAM_ID!,
			error: miningPoolError,
		}
		res
			.header('content-type', 'application/json')
			.send(JSON.stringify(data, psycheJsonReplacer))
	})

	fastify.get('/runs', (_req, res) => {
		const runs: ApiGetRuns = {
			runs: coordinator.dataStore.getRunSummaries(),
			error: coordinatorError,
		}

		res
			.header('content-type', 'application/json')
			.send(JSON.stringify(runs, psycheJsonReplacer))
	})

	fastify.get(
		'/run/:runId',
		(req: FastifyRequest<{ Params: { runId?: string } }>, res) => {
			const isStreamingRequest = req.headers.accept?.includes(
				'application/x-ndjson'
			)
			const { runId } = req.params

			const matchingRun = runId
				? coordinator.dataStore.getRunDataById(runId)
				: null
			const data: ApiGetRun = {
				run: matchingRun,
				error: coordinatorError,
				isOnlyRun: coordinator.dataStore.getRunSummaries().length === 1,
			}

			// set header for streaming/non
			res.header(
				'content-type',
				isStreamingRequest ? 'application/x-ndjson' : 'application/json'
			)

			if (!isStreamingRequest || !matchingRun) {
				res.send(JSON.stringify(data, psycheJsonReplacer))
				return
			}

			const id = matchingRun.info.id
			let listeners = liveRunListeners.get(id)
			if (!listeners) {
				listeners = new Set()
				liveRunListeners.set(id, listeners)
			}

			// start streaming newline-delimited json
			const stream = new PassThrough()
			res.send(stream)

			function sendRunData(runData: RunData) {
				console.log('sending run data ')
				const data: ApiGetRun = {
					run: runData,
					error: coordinatorError,
					isOnlyRun: coordinator.dataStore.getRunSummaries().length === 1,
				}
				stream.write(JSON.stringify(data, psycheJsonReplacer) + '\n')
			}

			// send the initial run data to populate the UI
			sendRunData(matchingRun)

			// this listener will be called every time we see a state change.
			listeners.add(sendRunData)

			// when the req closes, stop sending them updates
			req.socket.on('close', () => {
				listeners.delete(sendRunData)
				stream.end()
			})
		}
	)

	fastify.get('/status', async () => {
		return {
			commit: process.env.GITCOMMIT ?? '???',
			initTime,
			coordinator: {
				status: coordinatorError ? coordinatorError.toString() : 'ok',
				trackedRuns: coordinator.dataStore
					.getRunSummaries()
					.map((r) => ({ id: r.id, status: r.status })),
				chain: {
					chainSlotHeight: await coordinatorRpc.getSlot('confirmed'),
					indexedSlot:
						coordinator.dataStore.lastUpdate().highestSignature?.slot ?? 0,
					programId:
						process.env.COORDINATOR_PROGRAM_ID ?? coordinatorIdl.address,
					networkGenesis: await coordinatorRpc.getGenesisHash(),
				},
			},
			miningPool: {
				status: miningPoolError ? miningPoolError.toString() : 'ok',
				chain: {
					chainSlotHeight: await miningPoolRpc.getSlot('confirmed'),
					indexedSlot:
						miningPool.dataStore.lastUpdate().highestSignature?.slot ?? 0,
					programId:
						process.env.MINING_POOL_PROGRAM_ID ?? miningPoolIdl.address,
					networkGenesis: await miningPoolRpc.getGenesisHash(),
				},
			},
		} satisfies IndexerStatus
	})

	await fastify.listen({ port: 3000 })
}
main()
