import { PublicKey } from '@solana/web3.js'
import { readFileSync } from 'fs'
import { writeFile } from 'fs/promises'
import { PsycheCoordinator } from 'psyche-deserialize-zerocopy-wasm'
import {
	RunSummary,
	RunData,
	psycheJsonReviver,
	psycheJsonReplacer,
	Metrics,
	OverTime,
	ContributionInfo,
} from 'shared'
import {
	PsycheMiningPoolAccount,
	WitnessEvalResult,
	WitnessMetadata,
} from './idlTypes.js'
import path from 'path'

export interface ChainDataStore {
	lastProcessedSlot(): number

	/// flush to disk
	sync(lastProcessedSlot: number): Promise<void>
}

export interface CoordinatorDataStore extends ChainDataStore {
	updateRun(
		pubkey: string,
		newState: PsycheCoordinator,
		blockTimestamp: number
	): void
	setRunStatus(pubkey: string, paused: boolean): void
	witnessRun(pubkey: string, witness: WitnessMetadata): void
	destroyRun(pubkey: string): void

	getRunSummaries(): RunSummary[]
	getRunData(runId: string): RunData | null
}

export interface MiningPoolDataStore extends ChainDataStore {
	setFundingData(data: PsycheMiningPoolAccount): void
	setUserAmount(address: string, amount: bigint): void
	setCollateralInfo(mintAddress: string, decimals: number): void;
	hasCollateralInfo(): boolean
	getContributionInfo(): Omit<ContributionInfo, 'miningPoolProgramId'>
}

export class FlatFileMiningPoolDataStore implements MiningPoolDataStore {
	#lastSlot: number = -1
	#data: {
		totalDepositedCollateralAmount: bigint
		maxDepositCollateralAmount: bigint
		collateral: {
			mintAddress: string
			decimals: number
		} | null
		userDeposits: Map<string, bigint>
	} = {
		collateral: null,
		maxDepositCollateralAmount: 0n,
		totalDepositedCollateralAmount: 0n,
		userDeposits: new Map(),
	}
	#db: string

	constructor(dir: string) {
		this.#db = path.join(dir, './mining-pool-db.json')

		console.log('loading mining pool db from disk...')
		try {
			const { lastSlot, data } = JSON.parse(
				readFileSync(this.#db, 'utf-8'),
				psycheJsonReviver
			)
			this.#lastSlot = lastSlot
			this.#data = data
			console.log(`loaded DB from disk at slot ${this.#lastSlot}`)
		} catch (err) {
			console.warn('failed to load previous DB from disk: ', err)
		}
	}
	

	setFundingData(data: PsycheMiningPoolAccount): void {
		this.#data.maxDepositCollateralAmount = BigInt(data.maxDepositCollateralAmount.toString())
		this.#data.totalDepositedCollateralAmount = BigInt(data.totalDepositedCollateralAmount.toString())
	}

	setCollateralInfo(mintAddress: string, decimals: number) {
		this.#data.collateral = {
			mintAddress,
			decimals,
		}
	}

	setUserAmount(address: string, amount: bigint): void {
		this.#data.userDeposits.set(address, amount)
	}

	lastProcessedSlot(): number {
		return this.#lastSlot
	}

	async sync(lastProcessedSlot: number): Promise<void> {
		this.#lastSlot = lastProcessedSlot
		await writeFile(
			this.#db,
			JSON.stringify(
				{
					lastSlot: this.#lastSlot,
					data: this.#data,
				},
				psycheJsonReplacer
			)
		)
	}

	getContributionInfo(): Omit<ContributionInfo, 'miningPoolProgramId'> {
		const usersSortedByAmount = [...this.#data.userDeposits.entries()].sort(
			(a, b) => (a[1] > b[1] ? -1 : a[1] < b[1] ? 1 : 0)
		)
		return {
			totalDepositedCollateralAmount: this.#data.totalDepositedCollateralAmount,
			maxDepositCollateralAmount: this.#data.maxDepositCollateralAmount,
			users: usersSortedByAmount.map(([address, funding], i) => ({
				address,
				funding,
				rank: i + 1,
			})),
			collateralMintDecimals: this.#data.collateral?.decimals ?? 0,
			collateralMintAddress:
				this.#data.collateral?.mintAddress ?? 'UNKNOWN',
		}
	}
	hasCollateralInfo(): boolean {
		return this.#data.collateral !== null
	}
}

interface FlatFileCoordinatorRun {
	publicKey: PublicKey
	lastState: PsycheCoordinator
	paused: boolean
	firstSeenTimestamp: number
	lastSeenTimestamp: number
	witnessHistory: WitnessMetadata[]
}

export class FlatFileCoordinatorDataStore implements CoordinatorDataStore {
	#runs: Map<string, FlatFileCoordinatorRun> = new Map()
	#lastSlot: number = -1
	#db: string

	constructor(dir: string) {
		this.#db = path.join(dir, './coordinator-db.json')
		console.log('loading db from disk...')
		try {
			const { lastSlot, runs } = JSON.parse(
				readFileSync(this.#db, 'utf-8'),
				psycheJsonReviver
			)
			this.#lastSlot = lastSlot
			this.#runs = runs
			console.log(`loaded DB from disk at slot ${this.#lastSlot}`)
		} catch (err) {
			console.warn('failed to load previous DB from disk: ', err)
		}
	}

	async sync(lastProcessedSlot: number) {
		this.#lastSlot = lastProcessedSlot
		await writeFile(
			this.#db,
			JSON.stringify(
				{
					lastSlot: this.#lastSlot,
					runs: this.#runs,
				},
				psycheJsonReplacer
			)
		)
	}

	lastProcessedSlot() {
		return this.#lastSlot
	}

	updateRun(
		pubkey: string,
		newState: PsycheCoordinator,
		blockTimestamp: number
	) {
		const r = this.#runs.get(pubkey)

		if (r) {
			r.lastState = newState
			r.lastSeenTimestamp = Date.now()
		} else {
			this.#runs.set(pubkey, {
				paused: true,
				publicKey: new PublicKey(pubkey),
				lastState: newState,
				witnessHistory: [],
				firstSeenTimestamp: blockTimestamp,
				lastSeenTimestamp: Date.now(),
			})
		}
	}

	setRunStatus(pubkey: string, paused: boolean) {
		const r = this.#runs.get(pubkey)
		if (!r) {
			throw new Error(
				`Run ${pubkey} was never seen, but we're trying to update its pause status to "${paused}". This should be unreachable.`
			)
		}
		r.paused = paused
		r.lastSeenTimestamp = Date.now()
	}

	witnessRun(pubkey: string, witness: WitnessMetadata) {
		const r = this.#runs.get(pubkey)
		if (!r) {
			console.error(
				`Tried to add witness stats for run ${r} which isn't tracked!! This should be unreachable.`
			)
			return
		}
		r.lastSeenTimestamp = Date.now()
		r.witnessHistory.push(witness)
	}

	destroyRun(pubkey: string) {
		const r = this.#runs.get(pubkey)
		if (!r) {
			console.error(
				`Tried to destroy run ${r} which isn't tracked!! This should be unreachable.`
			)
			return
		}
		r.lastState.coordinator.run_state = 'Finished'
	}

	#getRunSummary(run: FlatFileCoordinatorRun): RunSummary {
		const c = run.lastState.coordinator

		const tokensPerSequence = c.model.LLM.max_seq_len
		const batchesPerStep = c.config.global_batch_size_end

		const tokensPerStep = tokensPerSequence * batchesPerStep

		const currentStep = c.progress.step
		const completedTokens = Number(currentStep) * tokensPerStep
		const totalSteps = c.config.total_steps
		const totalTokens = (totalSteps + 1) * tokensPerStep

		const summary: RunSummary = {
			arch: c.model.LLM.architecture,
			id: c.run_id,
			name: run.lastState.metadata.name,
			description: run.lastState.metadata.description,
			status:
				c.run_state === 'Finished'
					? {
							type: 'completed',
							at: new Date(run.lastSeenTimestamp),
						}
					: {
							type: 'active',
						},
			startTime: new Date(run.firstSeenTimestamp),
			totalTokens,
			completedTokens,
			size: run.lastState.metadata.num_parameters,
			type: 'vision', // TODO
		}
		return summary
	}

	getRunSummaries(): RunSummary[] {
		return [...this.#runs.values()].map(this.#getRunSummary)
	}

	getRunData(runId: string): RunData | null {
		const run = [...this.#runs.values()].find(
			(r) => r.lastState.coordinator.run_id === runId
		)
		if (!run) {
			return null
		}
		const info = this.#getRunSummary(run)

		const summary: Metrics = {
			bandwidth: run.witnessHistory.at(-1)?.bandwidth_per_sec ?? 0,
			loss: run.witnessHistory.at(-1)?.loss ?? Infinity,
			tokensPerSecond: run.witnessHistory.at(-1)?.tokens_per_sec ?? 0,
			evals: {},
		}

		const evals: Record<string, Array<{ step: number; value: number }>> = {}
		for (const r of run.witnessHistory) {
			// could be a bigint, could be a BN, kind of annoying. TODO fix somewhere else.
			const l =
				typeof r.evals.len === 'object' &&
				r.evals.len &&
				'toNumber' in r.evals.len
					? r.evals.len.toNumber()
					: Number(r.evals.len)
			for (const { name, value } of r.evals.data.slice(
				0,
				l
			) as WitnessEvalResult[]) {
				const firstZero = name[0].findIndex((v) => v === 0)
				const nameStr = Buffer.from(
					name[0].slice(0, firstZero)
				).toString('utf-8')
				if (!(nameStr in evals)) {
					evals[nameStr] = []
				}
				evals[nameStr].push({
					step: r.step,
					value,
				})
			}
		}
		const history: OverTime<Metrics> = {
			bandwidth: run.witnessHistory
				.map((h) => ({ step: h.step, value: h.bandwidth_per_sec }))
				.filter(goodNumber),
			loss: run.witnessHistory
				.map((h) => ({ step: h.step, value: h.loss }))
				.filter(goodNumber),
			tokensPerSecond: run.witnessHistory
				.map((h) => ({ step: h.step, value: h.tokens_per_sec }))
				.filter(goodNumber),
			evals,
		}
		return {
			info,
			metrics: {
				summary,
				history,
			},
		}
	}
}

function goodNumber({ value }: { value: number }): boolean {
	return Number.isFinite(value) && !Number.isNaN(value)
}
