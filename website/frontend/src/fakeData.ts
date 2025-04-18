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

function randomWalk(scale: number, start = 0, down = 0.9) {
	const numSteps = Math.floor(Math.random() * (2000 - 1000 + 1)) + 1000

	const walk = [{ step: 0, value: start }]

	let currentValue = start

	for (let i = 1; i <= numSteps; i++) {
		const movement = (Math.random() * 2 - down) * 0.005

		currentValue += movement
		currentValue = Math.max(Math.min(currentValue, 1), 0)

		walk.push({
			step: i,
			value: currentValue * scale,
		})
	}

	return walk
}

function randomClient(): RunRoundClient {
	return {
		pubkey: PublicKey.findProgramAddressSync(
			[new Uint8Array(3)],
			PublicKey.unique()
		)[0].toString(),
		witness:
			Math.random() > 0.66 ? 'done' : Math.random() > 0.5 ? 'waiting' : false,
	}
}

function randomPhase(): RunState {
	const seed = Math.random() * 0.5
	if (seed < 0.1) {
		return 'WaitingForMembers'
	} else if (seed < 0.2) {
		return 'Warmup'
	} else if (seed < 0.3) {
		return 'Cooldown'
	} else if (seed < 0.4) {
		return 'RoundTrain'
	} else if (seed < 0.5) {
		return 'RoundWitness'
	}
	return 'Uninitialized'
}
export const makeFakeRunData: Record<string, () => RunData> = {
	'run-001': () => {
		const numEpochs = Math.round(Math.random() * 300)
		const epoch = Math.round(Math.random() * numEpochs)
		const roundsPerEpoch = Math.round(Math.random() * 1000)
		const round = Math.round(Math.random() * roundsPerEpoch)
		return {
			info: fakeRunSummaries[0],
			metrics: {
				summary: {
					loss: 0.32 + Math.random() * 0.3,
					bandwidth: Math.random() * 128_000_000,
					tokensPerSecond: Math.random() * 128_000,
					evals: {
						accuracy: 0.83,
						precision: 0.79,
						recall: 0.85,
					},
				},
				history: {
					loss: randomWalk(1),
					bandwidth: randomWalk(1000000),
					tokensPerSecond: randomWalk(100000),
					evals: {
						accuracy: randomWalk(1),
						precision: randomWalk(1),
						recall: randomWalk(1),
					},
				},
			},
			state: {
				phase: randomPhase(),
				round,
				epoch,
				roundsPerEpoch,
				numEpochs,
				clients: Array.from({ length: Math.ceil(Math.random() * 16) }, () =>
					randomClient()
				),
			},
		}
	},
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
				loss: randomWalk(1, 1, 1.1),
				bandwidth: randomWalk(1_000_000),
				tokensPerSecond: randomWalk(100_000),
				evals: {
					accuracy: randomWalk(1),
					precision: randomWalk(1),
					recall: randomWalk(1),
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
