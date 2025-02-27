import {
	Program,
	Provider,
} from '@coral-xyz/anchor'
import {
	PublicKey,
} from '@solana/web3.js'
import {
	PsycheSolanaCoordinator,
} from 'shared'
import {
	load_coordinator_from_bytes,
	PsycheCoordinator,
} from 'psyche-deserialize-zerocopy-wasm'
import {
	PsycheCoordinatorInstructionsUnion,
	WitnessMetadata,
} from './idlTypes.js'
import {
	CoordinatorDataStore,
} from './dataStore.js'
import { startWatchChainLoop } from './chainLoop.js'

export async function startWatchCoordinatorChainLoop(
	dataStore: CoordinatorDataStore,
	coordinator: Program<PsycheSolanaCoordinator>,
	cancelled: { cancelled: boolean }
) {
	const resolver = new CoordinatorAddressResolver(coordinator)
	await startWatchChainLoop<PsycheCoordinatorInstructionsUnion>()(
		'coordinator',
		dataStore,
		coordinator,
		cancelled,
		{
			onStartCatchup() {
				return {
					runUpdatedInSlot: new Map<string, number>(),
					runCreatedAtTime: new Map<string, number>(),
					runsToDestroy: new Set<string>(),
					runPauseUpdates: new Map<string, boolean>(),
					runWitnessUpdates: [] as Array<[string, WitnessMetadata]>,
				}
			},
			onInstruction(
				tx,
				i,
				decoded,
				{
					runCreatedAtTime: runCreatedTimestamps,
					runPauseUpdates,
					runWitnessUpdates,
					runsToDestroy,
					runUpdatedInSlot: runsToUpdate,
				}
			) {
				// `i.accounts` is an array of all the accounts used in this TX.
				// we're using it to grab the address of the specific run we're updating.
				// this is different per instruction, but fixed order based on the IDL/contract code,
				// so it's safe to hardcode the index here.
				// for `runsToUpdate` we don't always have the actual address of the run data account,
				// so we instead find the "wrapper" account's address, and resolve it below.
				switch (decoded.name) {
					case 'init_coordinator': {
						const runAddr = i.accounts[2].toString()
						runCreatedTimestamps.set(
							runAddr,
							// solana timestamps are in second, pad out to ms.
							+(
								tx.blockTime?.toString().padEnd(13, '0') ??
								Date.now()
							)
						)
						runsToUpdate.set(runAddr, tx.slot)
						break
					}

					case 'tick':
					case 'update_coordinator_config_model':
					case 'join_run': {
						const runPubkey = i.accounts[1].toString()
						runsToUpdate.set(runPubkey, tx.slot)
						break
					}
					case 'set_paused': {
						const runPubkey = i.accounts[2].toString()
						runPauseUpdates.set(runPubkey, decoded.data.paused)
						break
					}
					case 'free_coordinator': {
						const runPubkey = i.accounts[1].toString()
						runsToDestroy.add(runPubkey)
						break
					}
					case 'witness': {
						const runPubkey = i.accounts[2].toString()
						runWitnessUpdates.push([
							runPubkey,
							decoded.data.metadata,
						] as const)
						break
					}

					default: {
						console.warn(
							`Skipping decoded instruction ${decoded.name}`
						)
						break
					}
				}
			},
			async onDoneCatchup(
				store,
				{
					runsToDestroy,
					runCreatedAtTime: runCreatedTimestamps,
					runPauseUpdates,
					runWitnessUpdates,
					runUpdatedInSlot: runsToUpdate,
				}
			) {
				const newRunStates = await Promise.all(
					[...runsToUpdate.entries()].map(async ([pubkey, slot]) => {
						try {
							const realPubkey = await resolver.resolve(
								new PublicKey(pubkey)
							)
							const timestamp =
								runCreatedTimestamps.get(pubkey) ?? Date.now()
							return [
								realPubkey.toString(),
								await getRunCoordinatorState(
									coordinator.provider,
									realPubkey
								),
								timestamp,
							] as const
						} catch (err) {
							throw new Error(
								`Failed to update run at key ${pubkey}, last seen at slot ${slot}: ${err}`
							)
						}
					})
				)

				for (const [
					pubkey,
					coordinatorState,
					timestamp,
				] of newRunStates) {
					store.updateRun(pubkey, coordinatorState, timestamp)
				}
				for (const [pubkey, paused] of runPauseUpdates) {
					store.setRunStatus(pubkey, paused)
				}
				for (const pubkey of runsToDestroy) {
					store.destroyRun(pubkey)
				}

				for (const [pubkey, witness] of runWitnessUpdates) {
					store.witnessRun(pubkey, witness)
				}
			},
		},
		5_000
	)
}

// coordinators have two accounts;
// a BorshSer/De account that looks like this:
// {
//   ...,
//  account: pubkey
// }
// and a non ser/de account that contains the real coordinator state,
// located at the address referred to in the wrapper account.
// this class exists to resolve the mapping of wrapper account -> real account,
// and cache the results so we don't have to repeat the lookup if we already have it.
class CoordinatorAddressResolver {
	#addresses: Map<string, PublicKey | Promise<PublicKey>> = new Map()
	#coordinator: Program<PsycheSolanaCoordinator>

	constructor(coordinator: Program<PsycheSolanaCoordinator>) {
		this.#coordinator = coordinator
	}

	resolve(parentAddress: PublicKey): Promise<PublicKey> | PublicKey {
		const parentAddressString = parentAddress.toString()
		const resolvedAddress = this.#addresses.get(parentAddressString)

		// we want to only do one fetch per addr ever, so we ensure to return an in-progress req if there is one.

		if (resolvedAddress !== undefined) {
			return resolvedAddress
		}

		const fetchPromise = this.#coordinator.account.coordinatorInstance
			.fetch(parentAddress)
			.then(({ coordinatorAccount }) => {
				this.#addresses.set(parentAddressString, coordinatorAccount)
				console.log(
					`[resolver] resolved coordinator address from ${parentAddressString} to ${coordinatorAccount}`
				)
				return coordinatorAccount
			})

		this.#addresses.set(parentAddressString, fetchPromise)

		return fetchPromise
	}
}

async function getRunCoordinatorState(
	provider: Provider,
	runPubkey: PublicKey
): Promise<PsycheCoordinator> {
	const accountInfo =
		await provider.connection.getParsedAccountInfo(runPubkey)
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
		return state
	} catch (err) {
		throw new Error(
			`Failed to deserialize run at address ${runPubkey}: ${err}`
		)
	}
}