import { ContributionInfo, IndexerStatus, RunData, RunSummary } from 'shared'

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
					at: new Date(Date.now() - 1000 * 60 * 60 * 24),
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
		name: 'Vision Model Alpha',
		description: 'Training a vision model to recognize everyday objects',
		status: { type: 'active' },
		startTime: new Date(Date.now() - 1000 * 60 * 60 * 24 * 14), // 2 weeks ago
		totalTokens: 100000,
		completedTokens: 65000,
		size: BigInt('1000000000'),
		arch: 'HfLlama',
		type: 'vision',
	},
	{
		id: 'run-002',
		name: 'Text Assistant Beta',
		description: 'Assistant-like model with reasoning capabilities',
		status: { type: 'active' },
		startTime: new Date(Date.now() - 1000 * 60 * 60 * 24 * 3), // 3 days ago
		totalTokens: 200000,
		completedTokens: 0,
		size: BigInt('2000000000'),
		arch: 'HfLlama',
		type: 'text',
	},
	{
		id: 'run-003',
		name: 'Small Language Model',
		description: 'Compact text model for edge devices',
		status: {
			type: 'completed',
			at: new Date(Date.now() - 1000 * 60 * 60 * 24),
		}, // 1 day ago
		startTime: new Date(Date.now() - 1000 * 60 * 60 * 24 * 10), // 10 days ago
		totalTokens: 50000,
		completedTokens: 50000,
		size: BigInt('500000000'),
		arch: 'HfLlama',
		type: 'text',
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

export const fakeRunData: Record<string, RunData> = {
	'run-001': {
		info: fakeRunSummaries[0],
		metrics: {
			summary: {
				loss: 0.32,
				bandwidth: 128.5,
				tokensPerSecond: 245.7,
				evals: {
					accuracy: 0.83,
					precision: 0.79,
					recall: 0.85,
				},
			},
			history: {
				loss: randomWalk(1),
				bandwidth: randomWalk(1_000_000),
				tokensPerSecond: randomWalk(100_000),
				evals: {
					accuracy: randomWalk(1),
					precision: randomWalk(1),
					recall: randomWalk(1),
				},
			},
		},
	},
	'run-002': {
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
	},
	'run-003': {
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
	},
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
