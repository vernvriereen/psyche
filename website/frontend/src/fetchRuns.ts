import {
	psycheJsonReviver,
	IndexerStatus,
	ApiGetRuns,
	ApiGetRun,
	ApiGetContributionInfo,
} from 'shared'
import {
	fakeContributionInfo,
	fakeIndexerStatus,
	makeFakeRunData,
	fakeRunSummaries,
} from './fakeData.js'

const { protocol, hostname } = window.location
const port = import.meta.env.VITE_BACKEND_PORT ?? window.location.port
const origin = `${protocol}//${hostname}${port ? `:${port}` : ''}`

let path = import.meta.env.VITE_BACKEND_PATH ?? '/'
path = path.startsWith('/') ? path.substring(1) : path

if (path && !path.startsWith('/')) {
	path = `/${path}`
}

const BACKEND_URL = `${origin}${path}`

function psycheJsonFetch(path: string) {
	return fetch(`${BACKEND_URL}/${path}`)
		.then(async (r) => [r, await r.text()] as const)
		.then(([r, text]) => {
			if (r.status !== 200) {
				throw new Error(`Failed to fetch ${path}: ${text}`)
			}
			return text
		})
		.then((text) => JSON.parse(text, psycheJsonReviver))
}

export async function fetchStatus(): Promise<IndexerStatus> {
	return import.meta.env.VITE_FAKE_DATA
		? fakeIndexerStatus
		: psycheJsonFetch('status')
}

export async function fetchRuns(): Promise<ApiGetRuns> {
	return import.meta.env.VITE_FAKE_DATA
		? ({
				runs: fakeRunSummaries,
				totalTokens: 1_000_000_000n,
				totalTokensPerSecondActive: 23_135_234n,
			} satisfies ApiGetRuns)
		: psycheJsonFetch('runs')
}

export async function fetchContributions(): Promise<ApiGetContributionInfo> {
	return import.meta.env.VITE_FAKE_DATA
		? (fakeContributionInfo satisfies ApiGetContributionInfo)
		: psycheJsonFetch('contributionInfo')
}

interface DecodeState {
	buffer: string
	decoder: TextDecoder
}

function makeDecodeState(): DecodeState {
	return {
		buffer: '',
		decoder: new TextDecoder('utf-8'),
	}
}

export async function fetchRunStreaming(
	runId: string,
	indexStr: string
): Promise<ReadableStream<ApiGetRun>> {
	if (import.meta.env.VITE_FAKE_DATA) {
		const seed = Math.random() * 1_000_000_000
		try {
			const index = Number.parseInt(indexStr ?? '0')
			if (`${index}` !== indexStr) {
				throw new Error(`Invalid index ${indexStr}`)
			}
			return new ReadableStream<ApiGetRun>({
				async start(controller) {
					let i = 0
					while (true) {
						controller.enqueue({
							run: makeFakeRunData[runId](seed, i, index),
							isOnlyRun: false,
						})
						const nextFakeDataDelay = 1000 + Math.random() * 1000
						await new Promise((r) => setTimeout(r, nextFakeDataDelay))
						i++
					}
				},
			})
		} catch (err) {
			return new ReadableStream({
				async start(controller) {
					controller.close()
				},
			})
		}
	}

	console.log('opening run stream for', runId)
	let { reader, decodeState } = await openRunStream(runId, indexStr)

	return new ReadableStream<ApiGetRun>({
		async start(controller) {
			const MAX_RECONNECT_ATTEMPTS = 5
			let reconnectAttempts = 0
			let reconnectDelay = 1000

			try {
				while (true) {
					const nextRun = await getOneRunFromStream(decodeState, reader)
					if (nextRun) {
						decodeState = nextRun.decodeState
						controller.enqueue(nextRun.parsedRun)
						continue
					}

					console.log('closing reader')

					await reader.cancel()

					// we failed to fetch a run because the stream ended - let's reconnect
					if (reconnectAttempts < MAX_RECONNECT_ATTEMPTS) {
						console.log(
							`Stream ended, attempting to reconnect (${reconnectAttempts + 1}/${MAX_RECONNECT_ATTEMPTS})...`
						)
						reconnectAttempts++
						await new Promise((resolve) => setTimeout(resolve, reconnectDelay))
						reconnectDelay = Math.min(reconnectDelay * 2, 10000)

						try {
							const newStream = await openRunStream(runId, indexStr)
							reader = newStream.reader
							decodeState = newStream.decodeState

							// if we opened a new stream successfully, we're good to go. reset our reconnect attempts
							reconnectAttempts = 0
							reconnectDelay = 1000
							continue // and start reading from the new connection
						} catch (reconnectError) {
							console.error('Failed to reconnect:', reconnectError)
							throw reconnectError
						}
					} else {
						console.log(
							'Maximum reconnection attempts reached, closing stream.'
						)
						break
					}
				}
			} catch (error) {
				console.error('Stream processing error:', error)
				controller.error(error)
			} finally {
				controller.close()
			}
		},
		cancel(reason) {
			reader.cancel(reason)
		},
	})
}

async function getOneRunFromStream(
	decodeState: DecodeState,
	reader: ReadableStreamDefaultReader<Uint8Array<ArrayBufferLike>>
) {
	let parsedRun: ApiGetRun | null = null
	while (!parsedRun) {
		const { value, done } = await reader.read()

		if (done) {
			return null
		}

		decodeState.buffer += decodeState.decoder.decode(value, { stream: true })
		const lines = decodeState.buffer.split('\n')

		if (lines.length > 1) {
			// we have at least one complete line, so one full JSON object
			const firstLine = lines[0].trim()
			if (firstLine) {
				parsedRun = JSON.parse(firstLine, psycheJsonReviver) as ApiGetRun
				decodeState.buffer = lines.slice(1).join('\n')
			} else {
				decodeState.buffer = lines.slice(1).join('\n')
			}
		}
	}
	return { parsedRun, decodeState }
}

async function openRunStream(runId: string, indexStr: string) {
	const response = await fetch(`${BACKEND_URL}/run/${runId}/${indexStr}`, {
		headers: {
			Accept: 'application/x-ndjson',
		},
	})

	if (!response.ok || !response.body) {
		throw new Error('Failed to fetch run data')
	}
	if (
		!(response.headers.get('Content-Type') ?? 'missing').includes(
			'application/x-ndjson'
		)
	) {
		throw new Error(
			`Invalid content type on response: expected "application/x-ndjson", got "${response.headers.get('Content-Type')}"`
		)
	}

	return { reader: response.body.getReader(), decodeState: makeDecodeState() }
}
