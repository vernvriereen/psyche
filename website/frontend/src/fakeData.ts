import { PublicKey } from '@solana/web3.js'
import {
	ContributionInfo,
	IndexerStatus,
	RunData,
	RunRoundClient,
	RunState,
	RunSummary,
} from 'shared'

export const fakeIndexerStatus: IndexerStatus = {
	initTime: Date.now() - 1000 * 60 * 60 * 24 * 7, // 1 week ago
	commit: 'fake data',
	coordinator: {
		status: 'ok',
		chain: {
			chainSlotHeight: 123456,
			indexedSlot: 123450,
			programId: '0x1234567890abcdef1234567890abcdef12345678',
			networkGenesis:
				'0xdeadbeefcafebabe0123456789abcdef0123456789abcdef0123456789abcdef',
		},
		trackedRuns: [
			{ id: 'run-001', status: { type: 'active' } },
			{ id: 'run-002', status: { type: 'funding' } },
			{
				id: 'run-003',
				status: {
					type: 'completed',
					at: {
						slot: 12345n,
						time: new Date(Date.now() - 1000 * 60 * 60 * 24),
					},
				},
			},
		],
	},
	miningPool: {
		status: 'ok',
		chain: {
			chainSlotHeight: 123456,
			indexedSlot: 123450,
			programId: '0x9876543210fedcba9876543210fedcba98765432',
			networkGenesis:
				'0xdeadbeefcafebabe0123456789abcdef0123456789abcdef0123456789abcdef',
		},
	},
}

export const fakeRunSummaries: RunSummary[] = [
	{
		id: 'run-001',
		index: 0,
		name: 'Vision Model Alpha',
		description: 'Training a vision model to recognize everyday objects',
		status: { type: 'active' },
		startTime: {
			slot: 12345n,
			time: new Date(Date.now() - 1000 * 60 * 60 * 24 * 14),
		}, // 2 weeks ago
		totalTokens: 100000n,
		completedTokens: 65000n,
		size: BigInt('1000000000'),
		arch: 'HfLlama',
		type: 'vision',
		pauseHistory: [],
	},
	{
		id: 'run-002',
		index: 0,
		name: 'Text Assistant Beta',
		description: 'Assistant-like model with reasoning capabilities',
		status: { type: 'active' },
		startTime: {
			slot: 12345n,
			time: new Date(Date.now() - 1000 * 60 * 60 * 24 * 3),
		}, // 3 days ago
		totalTokens: 200000n,
		completedTokens: 0n,
		size: BigInt('2000000000'),
		arch: 'HfLlama',
		type: 'text',
		pauseHistory: [],
	},
	{
		id: 'run-003',
		index: 0,
		name: 'Small Language Model',
		description: 'Compact text model for edge devices',
		status: {
			type: 'completed',
			at: {
				slot: 12345n,
				time: new Date(Date.now() - 1000 * 60 * 60 * 24),
			},
		}, // 1 day ago
		startTime: {
			slot: 12345n,
			time: new Date(Date.now() - 1000 * 60 * 60 * 24 * 10),
		}, // 10 days ago
		totalTokens: 50000n,
		completedTokens: 50000n,
		size: BigInt('500000000'),
		arch: 'HfLlama',
		type: 'text',
		pauseHistory: [],
	},
]

export const makeFakeRunData: Record<
	string,
	(seed?: number, step?: number) => RunData
> = {
	'run-001': makeFakeRunDataSeeded,
	'run-002': () => ({
		info: fakeRunSummaries[1],
		metrics: {
			summary: {
				loss: 0.0,
				bandwidth: 0.0,
				tokensPerSecond: 0.0,
				evals: {},
			},
			history: {
				loss: [],
				bandwidth: [],
				tokensPerSecond: [],
				evals: {},
			},
		},
	}),
	'run-003': () => ({
		info: fakeRunSummaries[2],
		metrics: {
			summary: {
				loss: 0.18,
				bandwidth: 156.3,
				tokensPerSecond: 320.4,
				evals: {
					accuracy: 0.91,
					precision: 0.89,
					recall: 0.9,
				},
			},
			history: {
				loss: randomWalk(1, 1, 1, 1.1),
				bandwidth: randomWalk(1, 1_000_000),
				tokensPerSecond: randomWalk(1, 100_000),
				evals: {
					accuracy: randomWalk(1, 1),
					precision: randomWalk(1, 1),
					recall: randomWalk(1, 1),
				},
			},
		},
	}),
}

export const fakeContributionInfo: ContributionInfo = {
	collateralMintDecimals: 6,
	totalDepositedCollateralAmount: 1250000000000n,
	maxDepositCollateralAmount: 2500000000000n,
	users: [
		{
			rank: 1,
			address: 'abc1234567890abcdef1234567890abcdef123456',
			funding: 500000000000n,
		},
		{
			rank: 2,
			address: 'def9876543210fedcba9876543210fedcba987654',
			funding: 300000000000n,
		},
		{
			rank: 3,
			address: '123abcdef0123456789abcdef0123456789abcdef',
			funding: 250000000000n,
		},
		{
			rank: 4,
			address: '456fedcba9876543210fedcba9876543210fedcba',
			funding: 120000000000n,
		},
		{
			rank: 5,
			address: '789abcdef0123456789abcdef0123456789abcdef',
			funding: 80000000000n,
		},
	],
	collateralMintAddress: 'N/A',
	miningPoolProgramId: 'N/A',
}

function makeFakeRunDataSeeded(seed = 1, step = 0) {
	const seededRandom = createSeededRandom(seed)

	const numEpochs = Math.round(seededRandom() * 300) + 10
	const roundsPerEpoch = Math.round(seededRandom() * 100) + 50
	const minClients = Math.round(seededRandom() * 10) + 2
	const totalClients = Math.round(seededRandom() * 10) + minClients

	const stepsPerEpoch = roundsPerEpoch + 2 // +2 for warmup and cooldown
	const currentEpoch = Math.min(Math.floor(step / stepsPerEpoch), numEpochs - 1)
	const epochStep = step % stepsPerEpoch

	const clients = Array.from({ length: totalClients }, (_, i) => {
		const basePubkey = PublicKey.findProgramAddressSync(
			[new Uint8Array([i, seed])],
			PublicKey.default
		)[0].toString()
		return {
			pubkey: basePubkey,
			witness: false,
		} as RunRoundClient
	})

	let phase: RunState = 'Uninitialized'
	let round = 0

	if (epochStep === 0) {
		phase = 'WaitingForMembers'

		// calculate how many clients have joined based on the step
		const joinedClients = Math.min(
			Math.max(
				1,
				Math.floor((minClients * (step + 1)) / (stepsPerEpoch * 0.2))
			),
			minClients
		)

		// only use the first 'joinedClients' clients
		clients.forEach((_, i) => {
			if (i >= joinedClients) {
				clients.splice(i)
			}
		})
	} else if (epochStep === 1) {
		phase = 'Warmup'
		round = 0
	} else if (epochStep === stepsPerEpoch - 1) {
		phase = 'Cooldown'
		round = roundsPerEpoch
	} else {
		// Training rounds - alternating between RoundTrain and RoundWitness
		round = epochStep - 1 // Adjust for warmup
		const isTraining = epochStep % 2 === 0
		phase = isTraining ? 'RoundTrain' : 'RoundWitness'

		clients.forEach((client, i) => {
			const clientSeedRandom = createSeededRandom(seed + i + epochStep)
			const isWitness = clientSeedRandom() > 0.5
			if (isTraining) {
				const state = clientSeedRandom()
				client.witness = isWitness ? (state > 0.5 ? 'done' : 'waiting') : false
			} else {
				client.witness = isWitness ? 'done' : false
			}
		})
	}

	return {
		info: fakeRunSummaries[0],
		metrics: {
			summary: {
				loss: 0.32 + seededRandom() * 0.3,
				bandwidth: seededRandom() * 128_000_000,
				tokensPerSecond: seededRandom() * 128_000,
				evals: {
					accuracy: 0.83,
					precision: 0.79,
					recall: 0.85,
				},
			},
			history: {
				loss: randomWalk(seed, 1, undefined, undefined, step),
				bandwidth: randomWalk(seed, 1000000, undefined, undefined, step),
				tokensPerSecond: randomWalk(seed, 100000, undefined, undefined, step),
				evals: {
					accuracy: randomWalk(seed, 1, undefined, undefined, step),
					precision: randomWalk(seed, 1, undefined, undefined, step),
					recall: randomWalk(seed, 1, undefined, undefined, step),
				},
			},
		},
		state: {
			phase,
			phaseStartTime: new Date(Date.now() - seededRandom() * 2_000),
			round,
			epoch: currentEpoch,
			clients,
			config: {
				cooldownTime: 5_000,
				maxRoundTrainTime: 5_000,
				warmupTime: 3_000,
				roundWitnessTime: 2_000,
				minClients,
				roundsPerEpoch,
				numEpochs,
			},
		},
	}
}

function createSeededRandom(seed: number) {
	return function () {
		const x = Math.sin(seed++) * 10000
		return x - Math.floor(x)
	}
}

function randomWalk(
	seed: number,
	scale: number,
	start = 0,
	down = 0.9,
	numStepsSet?: number
) {
	const seededRandom = createSeededRandom(seed)
	const numSteps =
		numStepsSet ?? Math.floor(seededRandom() * (2000 - 1000 + 1)) + 1000

	const walk = [{ step: 0, value: start }]

	let currentValue = start

	for (let i = 1; i <= numSteps; i++) {
		const movement = (seededRandom() * 2 - down) * 0.005

		currentValue += movement
		currentValue = Math.max(Math.min(currentValue, 1), 0)

		walk.push({
			step: i,
			value: currentValue * scale,
		})
	}

	return walk
}
