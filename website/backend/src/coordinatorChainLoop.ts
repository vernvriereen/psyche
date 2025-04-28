import { Program, Provider } from '@coral-xyz/anchor'
import { PublicKey } from '@solana/web3.js'
import { ChainTimestamp, getRunPDA, PsycheSolanaCoordinator } from 'shared'
import {
	load_coordinator_from_bytes,
	PsycheCoordinator,
} from 'psyche-deserialize-zerocopy-wasm'
import {
	PsycheCoordinatorInstructionsUnion,
	WitnessMetadata,
} from './idlTypes.js'
import { CoordinatorDataStore } from './dataStore.js'
import { startWatchChainLoop } from './chainLoop.js'
import { makeRetryPromise } from './rateLimit.js'

interface RunUpdates {
	created?: {
		runId: string
		timestamp: ChainTimestamp
	}
	destroyedAt?: ChainTimestamp
	pauseTimestamps: Array<['paused' | 'unpaused', ChainTimestamp]>
	witnessUpdates: Array<[WitnessMetadata, ChainTimestamp]>
	lastUpdated: {
		coordinatorAccountAddress: string
		timestamp: ChainTimestamp
	}
}

// When any change is made to a run, we'll insert the run info into its existing entry here, if it's not "destroyed"
// or, if it was destroyed, we mark it as "destroyed".

// If the last run in the list for a given key has not been destroyed,
// we'll fetch its state on-chain.

// This should ascribe witness updates, pauses, etc correctly to each run.
class UpdateManager {
	#runUpdates: Map<string, [RunUpdates, ...RunUpdates[]]> = new Map()

	createNewRun(
		pdaAddress: string,
		runId: string,
		coordinatorAccountAddress: string,
		timestamp: ChainTimestamp
	) {
		const runsAtThisAddress = this.#runUpdates.get(pdaAddress)
		const existingRun = runsAtThisAddress?.at(-1)

		if (existingRun && !existingRun.destroyedAt) {
			throw new Error(
				`Can't create a new run; run at ${pdaAddress} was *not* destroyed, but we're trying to create a new one!`
			)
		}

		// either there's nothing in the list, or the last run was destroyed
		// so we make a new one, track it, and return it
		const newRun: RunUpdates = {
			created: {
				runId,
				timestamp,
			},
			lastUpdated: { timestamp, coordinatorAccountAddress },
			pauseTimestamps: [],
			witnessUpdates: [],
		}
		if (runsAtThisAddress) {
			runsAtThisAddress.push(newRun)
		} else {
			this.#runUpdates.set(pdaAddress, [newRun])
		}
		return newRun
	}

	getAndTouchCurrentRun(
		pdaAddress: string,
		coordinatorAccountAddress: string,
		timestamp: ChainTimestamp
	) {
		// ensure the runs array is set
		if (!this.#runUpdates.has(pdaAddress)) {
			this.#runUpdates.set(pdaAddress, [
				{
					pauseTimestamps: [],
					witnessUpdates: [],
					lastUpdated: { timestamp, coordinatorAccountAddress },
				},
			])
		}

		const runsAtThisAddress = this.#runUpdates.get(pdaAddress)!
		const existingRun = runsAtThisAddress.at(-1)!

		if (existingRun.destroyedAt) {
			throw new Error(
				`Run at ${pdaAddress} was destroyed and a new one hasn't been created`
			)
		}
		if (
			existingRun.lastUpdated.coordinatorAccountAddress !==
			coordinatorAccountAddress
		) {
			throw new Error(
				`actual run addr is wrongggggg. got ${coordinatorAccountAddress}, expected ${existingRun.lastUpdated.coordinatorAccountAddress}`
			)
		}
		existingRun.lastUpdated = { timestamp, coordinatorAccountAddress }
		return existingRun
	}

	getRuns(): MapIterator<[string, [RunUpdates, ...RunUpdates[]]]> {
		return this.#runUpdates.entries()
	}
}

const COORDINATOR_LOOP_DELAY_MS = 5000

export async function startWatchCoordinatorChainLoop(
	dataStore: CoordinatorDataStore,
	coordinator: Program<PsycheSolanaCoordinator>,
	websocketRpcUrl: string,
	cancelled: { cancelled: boolean }
) {
	const getCoordinatorStateWithRetries = makeRetryPromise(
		getRunCoordinatorState
	)

	await startWatchChainLoop<PsycheCoordinatorInstructionsUnion>()(
		'coordinator',
		dataStore,
		coordinator,
		websocketRpcUrl,
		cancelled,
		{
			onStartCatchup() {
				return new UpdateManager()
			},
			onInstruction(tx, i, decoded, runUpdates) {
				const timestamp: ChainTimestamp = {
					slot: BigInt(tx.slot),
					// solana timestamps are in second, pad out to ms.
					time: new Date(
						+(tx.blockTime?.toString().padEnd(13, '0') ?? Date.now())
					),
				}

				// `i.accounts` is an array of all the accounts used in this TX.
				// we're using it to grab the address of the specific run we're updating.
				// this is different per instruction, but fixed order based on the IDL/contract code,
				// so it's safe to hardcode the index here.
				switch (decoded.name) {
					case 'init_coordinator': {
						const runAddr = i.accounts[1].toString()
						const coordinatorAddr = i.accounts[2].toString()
						const expectedRunAddr = getRunPDA(
							coordinator.programId,
							decoded.data.params.run_id
						)
						if (runAddr !== expectedRunAddr.toString()) {
							throw new Error(
								`Expected run addr ${expectedRunAddr.toString()}, but saw run addr ${runAddr}`
							)
						}
						runUpdates.createNewRun(
							runAddr,
							decoded.data.params.run_id,
							coordinatorAddr,
							timestamp
						)
						break
					}

					case 'update': {
						const runAddr = i.accounts[1].toString()
						const coordinatorAddr = i.accounts[2].toString()
						runUpdates.getAndTouchCurrentRun(
							runAddr,
							coordinatorAddr,
							timestamp
						)
						break
					}
					case 'tick': {
						const runAddr = i.accounts[1].toString()
						const coordinatorAddr = i.accounts[2].toString()
						runUpdates.getAndTouchCurrentRun(
							runAddr,
							coordinatorAddr,
							timestamp
						)
						break
					}
					case 'join_run': {
						const runAddr = i.accounts[2].toString()
						const coordinatorAddr = i.accounts[3].toString()
						runUpdates.getAndTouchCurrentRun(
							runAddr,
							coordinatorAddr,
							timestamp
						)
						break
					}
					case 'set_paused': {
						const runAddr = i.accounts[1].toString()
						const coordinatorAddr = i.accounts[2].toString()
						const run = runUpdates.getAndTouchCurrentRun(
							runAddr,
							coordinatorAddr,
							timestamp
						)
						run.pauseTimestamps.push([
							decoded.data.paused ? 'paused' : 'unpaused',
							timestamp,
						])
						break
					}
					case 'free_coordinator': {
						const runAddr = i.accounts[2].toString()
						const coordinatorAddr = i.accounts[3].toString()
						const run = runUpdates.getAndTouchCurrentRun(
							runAddr,
							coordinatorAddr,
							timestamp
						)
						run.destroyedAt = timestamp
						break
					}
					case 'witness': {
						const runAddr = i.accounts[1].toString()
						const coordinatorAddr = i.accounts[2].toString()
						const run = runUpdates.getAndTouchCurrentRun(
							runAddr,
							coordinatorAddr,
							timestamp
						)
						run.witnessUpdates.push([decoded.data.metadata, timestamp])
						break
					}
					case 'checkpoint': {
						// todo checkpoint
						break
					}
					case 'health_check': {
						// todo health check?
						break
					}
					case 'set_future_epoch_rates': {
						// set rates??
						break
					}
					case 'warmup_witness': {
						// anything here?
						break
					}
					default: {
						throw new Error(
							`Skipping decoded instruction at slot ${tx.slot} ${JSON.stringify(decoded)}`
						)
					}
				}
			},
			async onDoneCatchup(store, runUpdates) {
				const allRuns = [...runUpdates.getRuns()]
				if (allRuns.length === 0) {
					return
				}
				const allRunsWithState: Array<
					readonly [
						string,
						Array<
							RunUpdates & {
								state?: [PsycheCoordinator, ChainTimestamp]
							}
						>,
					]
				> = await Promise.all(
					allRuns.map(([addr, runsAtThisAddr]) => {
						const latestRun = runsAtThisAddr.at(-1)!
						// if the run is currently destroyed, we can't fetch a state update for it,
						// since it's gone on-chain.
						const canFetchState = !latestRun.destroyedAt

						// this fetches the state for the last run in the list.
						if (canFetchState) {
							const { coordinatorAccountAddress } = latestRun.lastUpdated
							console.log(
								`[coordinator] fetching state for run ${addr}, whose coordinator PDA lives at ${coordinatorAccountAddress}, we saw updated at slot ${latestRun.lastUpdated.timestamp.slot}`
							)
							return getCoordinatorStateWithRetries(
								coordinator.provider,
								new PublicKey(coordinatorAccountAddress)
							).then(
								(runState) =>
									[
										addr,
										runsAtThisAddr.map((run, i) => {
											if (i === runsAtThisAddr.length - 1) {
												return {
													...run,
													state: runState,
												}
											}
											return run
										}),
									] as const
							)
						}
						return [addr, runsAtThisAddr] as const
					})
				)
				for (const [i, [pubkey, runs]] of allRunsWithState.entries()) {
					console.log(
						`[coordinator] applying update for run ${i + 1}/${allRunsWithState.length}`
					)
					for (const run of runs) {
						if (run.created) {
							store.createRun(
								pubkey,
								run.created.runId,
								run.created.timestamp,
								run.state?.[0]
							)
						}
						if (run.state) {
							store.updateRun(pubkey, run.state[0], run.state[1])
						}
						for (const [witness, timestamp] of run.witnessUpdates) {
							store.witnessRun(pubkey, witness, timestamp)
						}
						for (const [pause, timestamp] of run.pauseTimestamps) {
							store.setRunPaused(pubkey, pause === 'paused', timestamp)
						}
						if (run.destroyedAt) {
							store.destroyRun(pubkey, run.destroyedAt)
						}
					}
				}
			},
		},
		COORDINATOR_LOOP_DELAY_MS
	)
}

async function getRunCoordinatorState(
	provider: Provider,
	runPubkey: PublicKey
): Promise<[PsycheCoordinator, ChainTimestamp]> {
	const accountInfo = await provider.connection.getParsedAccountInfo(
		runPubkey,
		'confirmed'
	)
	if (!accountInfo.value?.data) {
		throw new Error('No data for run at address: ' + runPubkey)
	}

	if (!(accountInfo.value.data instanceof Buffer)) {
		throw new Error(
			'Data is not a buffer when loading run at address: ' + runPubkey
		)
	}
	try {
		const state = load_coordinator_from_bytes(accountInfo.value.data)
		return [
			state,
			{
				slot: BigInt(accountInfo.context.slot),
				time: new Date(),
			},
		]
	} catch (err) {
		throw new Error(`Failed to deserialize run at address ${runPubkey}: ${err}`)
	}
}
