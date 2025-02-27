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
import {
	ChainDataStore,
} from './dataStore.js'

export function startWatchChainLoop<D>(): <T, I extends Idl, S extends ChainDataStore>(
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
			console.debug(
				`[${name}] checking new TXs from ${lastProcessedSlot} to ${processUntilSlot}...`
			)

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
				const instr = tx.transaction.message.instructions
				for (const i of instr) {
					// instructions without a payload aren't useful to us.
					if (!('data' in i)) {
						continue
					}

					const rawDecoded = instructionCoder.decode(i.data, 'base58')
					// instructions that we can't decode with our IDL aren't useful to us.
					if (rawDecoded === null) {
						continue
					}

					try {
						// this is a bit of a "hope and pray" cast,
						// the IDL stuff is a bit of a nightmare to work with.
						process.onInstruction(tx, i, rawDecoded as D, state)
					} catch (err) {
						throw new Error(
							`failed to process instruction in ${program.programId} from TX in slot ${tx.slot}: ${err}`
						)
					}
				}
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
	const catchupTxs = []

	// start with the newest possible signature, and go back to the oldest one.
	let minProcessedSignature = undefined
	while (true) {
		const numAlreadyProcessed = catchupTxs.length
		const signatures = await provider.connection.getSignaturesForAddress(
			address,
			{
				minContextSlot: latestSlotHeight,
				before: minProcessedSignature,
			},
			'confirmed'
		)
		if (cancelled.cancelled) {
			return null
		}

		const signaturesAfterLastProcessedSlot = signatures.filter(
			(s) => s.slot > lastProcessedSlot
		)

		const maybeMoreSignatures = signatures.length === 1000

		if (signaturesAfterLastProcessedSlot.length != 0) {
			const signatures = signaturesAfterLastProcessedSlot.map(
				(s) => s.signature
			)
			console.log(
				`[${name}] fetching ${signaturesAfterLastProcessedSlot.length} sigs catching up from ${lastProcessedSlot} to ${minProcessedSignature ?? latestSlotHeight}`
			)

			const transactions: Array<ParsedTransactionWithMeta | null> = []
			for (const signature of signatures) {
				console.log(
					`[${name}] fetching sig ${transactions.length + catchupTxs.length}/${numAlreadyProcessed + signatures.length}${maybeMoreSignatures ? '+' : ''}...`
				)
				const tx = await provider.connection.getParsedTransaction(
					signature,
					{
						commitment: 'confirmed',
						maxSupportedTransactionVersion: 0,
					}
				)

				if (cancelled.cancelled) {
					return null
				}

				transactions.push(tx)
			}

			const nonNullTransactions = transactions.filter((t) => {
				const txParsed = t != null
				if (!txParsed) {
					console.warn(`Transaction failed to decode!!!`)
				}
				return txParsed
			})
			catchupTxs.push(...nonNullTransactions)

			// pick the lowest signature as our next iter's highest sig to start from,
			// so we'll iter further and further back into the past
			minProcessedSignature = nonNullTransactions.reduce(
				(min, curr) => {
					if ((curr.slot ?? Infinity) < (min?.slot ?? Infinity)) {
						return curr
					} else {
						return min
					}
				},
				null as null | ParsedTransactionWithMeta
			)?.transaction.signatures[0]
		}

		// if we've run out of signatures to process, or at lesat one signature has gone past our last processed slot, we're done.
		if (
			signatures.length === 0 ||
			signaturesAfterLastProcessedSlot.length != signatures.length
		) {
			break
		}
	}

	// ok here, we now have a list of TXs to the this account, from newest to oldest.
	// reverse into oldest -> newest, so can process in chronological order.
	catchupTxs.reverse()

	return catchupTxs
}