import {
	BorshInstructionCoder,
	Idl,
	Program,
	Provider,
} from '@coral-xyz/anchor'
import {
	ParsedTransactionWithMeta,
	PartiallyDecodedInstruction,
	PublicKey,
} from '@solana/web3.js'
import { ChainDataStore } from './dataStore.js'

export function startWatchChainLoop<D>(): <
	T,
	I extends Idl,
	S extends ChainDataStore,
>(
	name: string,
	dataStore: S,
	program: Program<I>,
	cancelled: { cancelled: boolean },
	process: {
		onStartCatchup(firstStateEver: boolean): T
		onInstruction(
			tx: ParsedTransactionWithMeta,
			instruction: PartiallyDecodedInstruction,
			decoded: D,
			state: T
		): void
		onDoneCatchup(dataStore: S, state: T): Promise<void>
	},
	delayMsBetweenUpdates?: number
) => Promise<void> {
	return async (
		name,
		dataStore,
		program,
		cancelled,
		process,
		delayMsBetweenUpdates = 1000
	) => {
		let lastProcessedSlot = dataStore.lastProcessedSlot()

		const instructionCoder = new BorshInstructionCoder(program.rawIdl)

		while (!cancelled.cancelled) {
			await new Promise((r) => setTimeout(r, delayMsBetweenUpdates))

			// always go a few slots behind so we don't get ourselves in trouble ;)
			const processUntilSlot =
				(await program.provider.connection.getSlot('confirmed')) - 10
			if (processUntilSlot === lastProcessedSlot) {
				continue
			}

			const catchupTxs = await catchupOnTxsToAddress(
				name,
				program.provider,
				program.programId,
				processUntilSlot,
				lastProcessedSlot,
				cancelled
			)

			if (catchupTxs === null) {
				// cancelled
				return
			}

			if (catchupTxs.length) {
				console.debug(
					`[${name}] updated from slot ${lastProcessedSlot} to slot ${processUntilSlot}.`
				)
			}

			const state = process.onStartCatchup(lastProcessedSlot === -1)

			for (const tx of catchupTxs) {
				if (catchupTxs.indexOf(tx) % 1000 === 0) {
					console.log(
						`[${name}] processing changes from tx ${catchupTxs.indexOf(tx)} / ${catchupTxs.length} (${((catchupTxs.indexOf(tx) / catchupTxs.length) * 100).toFixed(2)}%)`
					)
				}
				if (cancelled.cancelled) {
					return
				}
				const instr = tx.transaction.message.instructions
				for (const i of instr) {
					// instructions without a payload aren't useful to us.
					if (!('data' in i)) {
						continue
					}
					let rawDecoded
					try {
						rawDecoded = instructionCoder.decode(i.data, 'base58')	
					} catch (err) {
						// instructions that we can't decode with our IDL aren't useful to us. hopefully this doesn't break.
						console.warn(
							`Failed to process instruction from tx in slot ${tx.slot}. Attempting to continue. Maybe we have a different IDL than this was created with?`
						)
					}
					if (!rawDecoded) {
						continue
					}
					try {
						// this is a bit of a "hope and pray" cast,
						// the IDL stuff is a bit of a nightmare to work with.
						process.onInstruction(tx, i, rawDecoded as D, state)
						if (cancelled.cancelled) {
							break
						}
					} catch (err) {
						throw new Error(
							`failed to process instruction in ${program.programId} from TX in slot ${tx.slot}: ${err}`
						)
					}
				}
			}
			if (cancelled.cancelled) {
				break
			}
			lastProcessedSlot = processUntilSlot
			await process.onDoneCatchup(dataStore, state)
			await dataStore.sync(processUntilSlot)
		}
		console.info(`[${name}] chain loop was cancelled, exiting cleanly...`)
	}
}

async function catchupOnTxsToAddress(
	name: string,
	provider: Provider,
	address: PublicKey,
	latestSlotHeight: number,
	lastProcessedSlot: number,
	cancelled: { cancelled: boolean }
) {
	// start with the newest possible signature, and go back to the oldest one.
	let oldestSeenSignature: { signature: string; slot: number } | undefined =
		undefined
	const allSignatures = []
	while (true) {
		console.log(
			`[${name}] fetching sigs from slot ${lastProcessedSlot} to ${oldestSeenSignature?.slot ?? latestSlotHeight}: total ${allSignatures.length}`
		)
		const signatures = await provider.connection.getSignaturesForAddress(
			address,
			{
				minContextSlot: latestSlotHeight,
				before: oldestSeenSignature?.signature,
			},
			'confirmed'
		)

		if (cancelled.cancelled) {
			return null
		}

		const signaturesAfterLastProcessedSlot = signatures.filter(
			(s) => s.slot > lastProcessedSlot
		)

		// pick the last signature as our next iter's highest sig to start from,
		// so we'll iter further and further back into the past
		oldestSeenSignature = signaturesAfterLastProcessedSlot.at(-1)
		allSignatures.push(
			...signaturesAfterLastProcessedSlot.map((s) => s.signature)
		)

		// if we've run out of signatures to process, or at lesat one signature has gone past our last processed slot, we're done.
		if (
			signatures.length === 0 ||
			signaturesAfterLastProcessedSlot.length != signatures.length
		) {
			break
		}
	}

	// nice early exit :)
	if (allSignatures.length === 0) {
		return []
	}

	console.log(
		`[${name}] fetching ${allSignatures.length} transactions catching up from ${lastProcessedSlot} to ${oldestSeenSignature?.slot ?? latestSlotHeight}`
	)

	let completedCount = 0

	// fetch in parallel. the fetch rate limiter will limit this to something reasonable.
	const fetchPromises = allSignatures.map((signature) =>
		provider.connection
			.getParsedTransaction(signature, {
				commitment: 'confirmed',
				maxSupportedTransactionVersion: 0,
			})
			.then((tx) => {
				completedCount++
				console.log(
					`[${name}] fetched sig ${completedCount}/${allSignatures.length}, ${((completedCount / allSignatures.length) * 100).toFixed(2)}% ...`
				)

				if (cancelled.cancelled) {
					throw new Error('Operation cancelled')
				}

				if (!tx) {
					console.warn(
						`[${name}] Transaction failed to decode at signature ${signature}`
					)
				}

				return tx
			})
	)

	const transactions: Array<ParsedTransactionWithMeta | null> =
		await Promise.all(fetchPromises)

	const catchupTxs = transactions.filter((t) => !!t)
	// ok here, we now have a list of TXs to the this account, from newest to oldest.
	// reverse into oldest -> newest, so can process in chronological order.
	catchupTxs.reverse()

	return catchupTxs
}
