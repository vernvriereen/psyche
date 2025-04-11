import { readFileSync } from 'fs'
import { writeFile } from 'fs/promises'
import path from 'path'
import { PsycheCoordinator } from 'psyche-deserialize-zerocopy-wasm'
import {
	psycheJsonReviver,
	psycheJsonReplacer,
	RunSummary,
	RunData,
	Metrics,
	OverTime,
	ChainTimestamp,
	getRunPDA,
} from 'shared'
import { CoordinatorDataStore } from '../dataStore.js'
import { WitnessMetadata, WitnessEvalResult } from '../idlTypes.js'
import { PublicKey } from '@solana/web3.js'

type Witness = Omit<WitnessMetadata, 'evals'> & {
	evals: Array<{
		name: string
		value: number
	}>
}

interface RunHistory {
	runId: string
	createdAt: ChainTimestamp
	destroyedAt: ChainTimestamp | null
	lastUpdated: ChainTimestamp

	lastState: PsycheCoordinator | null
	pauseTimestamps: Array<['paused' | 'unpaused', ChainTimestamp]>
	witnessUpdates: Array<[Witness, ChainTimestamp]>
}

export class FlatFileCoordinatorDataStore implements CoordinatorDataStore {
	#runs: Map<string, RunHistory[]> = new Map()
	#lastSlot: number = -1
	#db: string
	#programId: PublicKey

	constructor(dir: string, programId: PublicKey) {
		this.#db = path.join(dir, './coordinator-db.json')
		this.#programId = programId
		console.log('loading coordinator db from disk...')
		try {
			const { lastSlot, runs, programId } = JSON.parse(
				readFileSync(this.#db, 'utf-8'),
				psycheJsonReviver
			)
			if (this.#programId.equals(programId)) {
				this.#lastSlot = lastSlot
				this.#runs = runs
				console.log(`loaded DB from disk at slot ${this.#lastSlot}`)
			} else {
				console.warn(
					`Program ID for coordinator changed from ${programId} in saved state to ${this.#programId} in args. **Starting from a fresh database**.`
				)
			}
		} catch (err) {
			console.warn('failed to load previous DB from disk: ', err)
		}
	}

	#getActiveRun(pubkey: string) {
		const lastRun = this.#runs.get(pubkey)?.at(-1)
		if (!lastRun) {
			throw new Error(
				`Tried to get active run ${pubkey}, but we have no runs recorded for that pubkey.`
			)
		}

		if (lastRun.destroyedAt) {
			throw new Error(
				`Tried to get active run ${pubkey}, but we saw it shut down at slot ${lastRun.destroyedAt.slot}, and we haven't seen a create since.`
			)
		}
		return lastRun
	}

	async sync(lastProcessedSlot: number) {
		this.#lastSlot = lastProcessedSlot
		await writeFile(
			this.#db,
			JSON.stringify(
				{
					lastSlot: this.#lastSlot,
					runs: this.#runs,
					programId: this.#programId,
				},
				psycheJsonReplacer
			)
		)
	}

	lastProcessedSlot() {
		return this.#lastSlot
	}

	createRun(
		pubkey: string,
		runId: string,
		eventTime: ChainTimestamp,
		// it's possible that we never get a state, if the run was created and destroyed while we're offline.
		newState?: PsycheCoordinator
	): void {
		if (!this.#runs.has(pubkey)) {
			this.#runs.set(pubkey, [])
		}
		const runsAtThisAddress = this.#runs.get(pubkey)!
		const lastKnownRun = runsAtThisAddress.at(-1)
		if (lastKnownRun && lastKnownRun.destroyedAt === null) {
			throw new Error(
				`Tried to create run ${pubkey}, but we have existing run at this address, created at slot ${lastKnownRun.createdAt.slot}`
			)
		}
		runsAtThisAddress.push({
			runId,
			createdAt: eventTime,
			destroyedAt: null,
			pauseTimestamps: [],
			lastUpdated: eventTime,
			witnessUpdates: [],
			lastState: newState ?? null,
		})
	}

	updateRun(
		pubkey: string,
		newState: PsycheCoordinator,
		eventTime: ChainTimestamp
	) {
		const lastRun = this.#getActiveRun(pubkey)
		lastRun.lastUpdated = eventTime
		lastRun.lastState = newState
	}

	setRunPaused(pubkey: string, paused: boolean, timestamp: ChainTimestamp) {
		const lastRun = this.#getActiveRun(pubkey)
		const newPauseState = paused ? 'paused' : 'unpaused'
		const lastPauseChange = lastRun.pauseTimestamps.at(-1)
		if (lastPauseChange?.[0] === newPauseState) {
			throw new Error(
				`Tried to set run ${pubkey} to pause state ${newPauseState} at slot ${timestamp.slot}, but it's already in that state from pause change at slot ${lastPauseChange[1].slot}.`
			)
		}
		lastRun.pauseTimestamps.push([newPauseState, timestamp])
	}

	witnessRun(
		pubkey: string,
		witness: WitnessMetadata,
		timestamp: ChainTimestamp
	) {
		const lastRun = this.#runs.get(pubkey)?.at(-1)
		if (!lastRun) {
			throw new Error(
				`Tried to get run ${pubkey}, but we have no runs recorded for that pubkey.`
			)
		}
		// we don't reallllllly care if it's shut down.
		lastRun.lastUpdated = timestamp

		// format evals to nice strings to save tons of space
		const { evals, ...restWitness } = witness

		// could be a bigint, could be a BN, kind of annoying. TODO fix somewhere else.
		const l =
			typeof evals.len === 'object' &&
			evals.len &&
			'toNumber' in evals.len
				? evals.len.toNumber()
				: Number(evals.len)
		const fixedEvals = []
		for (const { name, value } of evals.data.slice(
			0,
			l
		) as WitnessEvalResult[]) {
			const firstZero = name[0].findIndex((v) => v === 0)
			const nameStr = Buffer.from(name[0].slice(0, firstZero)).toString(
				'utf-8'
			)
			fixedEvals.push({
				name: nameStr,
				value,
			})
		}
		lastRun.witnessUpdates.push([
			{ ...restWitness, evals: fixedEvals },
			timestamp,
		])
	}

	destroyRun(pubkey: string, timestamp: ChainTimestamp) {
		const lastRun = this.#runs.get(pubkey)?.at(-1)
		if (!lastRun) {
			throw new Error(
				`Tried to get run ${pubkey}, but we have no runs recorded for that pubkey.`
			)
		}
		if (lastRun.destroyedAt !== null) {
			throw new Error(
				`Tried to destroy run ${pubkey}, but it's already marked as destroyed at slot ${lastRun.destroyedAt.slot} / time ${lastRun.destroyedAt.time}`
			)
		}
		lastRun.lastUpdated = timestamp
		lastRun.destroyedAt = timestamp
	}

	getRunSummaries(): RunSummary[] {
		return [...this.#runs.values()].flatMap((runs) =>
			runs.flatMap(makeRunSummary).filter((r) => !!r)
		)
	}

	getRunData(
		coordinatorInstancePdaAddress: PublicKey,
		index?: number
	): RunData | null {
		const runsAtThisAddress = this.#runs.get(
			coordinatorInstancePdaAddress.toString()
		)
		const run = runsAtThisAddress?.at(index ?? -1)
		if (!run) {
			return null
		}
		const realIndex = runsAtThisAddress!.indexOf(run)
		const info = makeRunSummary(run, realIndex)
		if (!info) {
			return null
		}

		const evals: Record<string, Array<{ step: number; value: number }>> = {}
		for (const [r] of run.witnessUpdates) {
			for (const { name, value } of r.evals) {
				if (!(name in evals)) {
					evals[name] = []
				}
				evals[name].push({
					step: r.step,
					value,
				})
			}
		}
		const history: OverTime<Metrics> = {
			bandwidth: run.witnessUpdates
				.map(([h]) => ({ step: h.step, value: h.bandwidth_per_sec }))
				.filter(goodNumber),
			loss: run.witnessUpdates
				.map(([h]) => ({ step: h.step, value: h.loss }))
				.filter(goodNumber),
			tokensPerSecond: run.witnessUpdates
				.map(([h]) => ({ step: h.step, value: h.tokens_per_sec }))
				.filter(goodNumber),
			evals,
		}

		const lastWitnessUpdate = run.witnessUpdates.at(-1)
		const summary: Metrics = {
			bandwidth: lastWitnessUpdate?.[0].bandwidth_per_sec ?? 0,
			loss: lastWitnessUpdate?.[0].loss ?? Infinity,
			tokensPerSecond: lastWitnessUpdate?.[0].tokens_per_sec ?? 0,
			evals: Object.fromEntries(
				Object.entries(evals)
					.map(([k, v]) => [k, v.at(-1)?.value] as const)
					.filter((x): x is [string, number] => x[1] !== undefined)
			),
		}
		return {
			info,
			metrics: {
				summary,
				history,
			},
		}
	}

	getRunDataById(runId: string, index?: number): RunData | null {
		const addr = getRunPDA(this.#programId, runId)
		return this.getRunData(addr, index)
	}
}

function goodNumber({ value }: { value: number }): boolean {
	return Number.isFinite(value) && !Number.isNaN(value)
}

function makeRunSummary(run: RunHistory, index: number): RunSummary | null {
	if (!run.lastState) {
		return null
	}
	const c = run.lastState.coordinator

	const tokensPerSequence = BigInt(c.model.LLM.max_seq_len)
	const batchSizeStart = BigInt(c.config.global_batch_size_start)
	const batchSizeEnd = BigInt(c.config.global_batch_size_end)
	const warmupTokens = c.config.global_batch_size_warmup_tokens
	const currentStep = BigInt(c.progress.step)
	const totalSteps = BigInt(c.config.total_steps)

	const completedTokens = calculateTokens(
		currentStep,
		tokensPerSequence,
		batchSizeStart,
		batchSizeEnd,
		warmupTokens
	)

	const totalTokens = calculateTokens(
		totalSteps,
		tokensPerSequence,
		batchSizeStart,
		batchSizeEnd,
		warmupTokens
	)

	const summary: RunSummary = {
		arch: c.model.LLM.architecture,
		id: c.run_id,
		index: index,
		name: run.lastState.metadata.name,
		description: run.lastState.metadata.description,
		status: run.destroyedAt
			? {
					type: 'completed',
					at: run.destroyedAt,
				}
			: c.run_state === 'Finished'
				? {
						type: 'completed',
						at: run.lastUpdated,
					}
				: {
						type:
							run.pauseTimestamps.at(-1)?.[0] === 'paused'
								? 'paused'
								: 'active',
					},
		startTime: run.createdAt,
		pauseHistory: run.pauseTimestamps,
		totalTokens,
		completedTokens,
		size: run.lastState.metadata.num_parameters,
		type: 'vision', // TODO
	}
	return summary
}

// linear warmup then constant at the warmup-end value is just a piecewise of area of trapezoid and area of rectangle!
function calculateTokens(
	step: bigint,
	tokensPerSequence: bigint,
	batchSizeStart: bigint,
	batchSizeEnd: bigint,
	warmupTokens: bigint
) {
	const avgBatchSizeDuringWarmup = (batchSizeStart + batchSizeEnd) / 2n
	const stepsForWarmup =
		warmupTokens / (tokensPerSequence * avgBatchSizeDuringWarmup)

	// trapezoid area (warmup phase)
	const trapezoidTokens =
		((step < stepsForWarmup ? step : stepsForWarmup) *
			tokensPerSequence *
			(batchSizeStart + batchSizeEnd)) /
		2n

	// rectangle area (post-warmup phase)
	const desiredPostWarmupSteps = step - stepsForWarmup
	const postWarmupSteps =
		desiredPostWarmupSteps > 0 ? desiredPostWarmupSteps : 0n
	const rectangleTokens = postWarmupSteps * tokensPerSequence * batchSizeEnd

	return trapezoidTokens + rectangleTokens
}
