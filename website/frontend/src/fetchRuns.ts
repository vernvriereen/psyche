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
		? ({ runs: fakeRunSummaries } satisfies ApiGetRuns)
		: psycheJsonFetch('runs')
}

export async function fetchContributions(): Promise<ApiGetContributionInfo> {
	return import.meta.env.VITE_FAKE_DATA
		? (fakeContributionInfo satisfies ApiGetContributionInfo)
		: psycheJsonFetch('contributionInfo')
}

export async function fetchRunStreaming(runId: string): Promise<{
	initialData: ApiGetRun
	stream: ReadableStream<ApiGetRun>
}> {
	if (import.meta.env.VITE_FAKE_DATA) {
		return {
			initialData: { run: makeFakeRunData[runId](), isOnlyRun: false },
			stream: new ReadableStream<ApiGetRun>({
				async start(controller) {
					while (true) {
						controller.enqueue({
							run: makeFakeRunData[runId](),
							isOnlyRun: false,
						})
						const nextFakeDataDelay = 1000 + Math.random() * 1000
						await new Promise((r) => setTimeout(r, nextFakeDataDelay))
					}
				},
			}),
		}
	}

	const response = await fetch(`${BACKEND_URL}/run/${runId}`, {
		headers: {
			Accept: 'application/x-ndjson',
		},
	})

	if (!response.ok || !response.body) {
		throw new Error('Failed to fetch run data')
	}
	if (response.headers.get('Content-Type') !== 'application/x-ndjson') {
		throw new Error(
			`Invalid content type on response: expected "application/x-ndjson", got "${response.headers.get('Content-Type')}"`
		)
	}

	// pull the run from the first chunk right away
	const reader = response.body.getReader()
	const decoder = new TextDecoder('utf-8')

	let buffer = ''

	// read until the first complete JSON object for initialData
	let initialData: ApiGetRun | null = null
	while (!initialData) {
		const { value, done } = await reader.read()

		if (done) {
			return {
				initialData: {
					isOnlyRun: false,
					run: null,
					error: new Error(
						'Failed to get initial data from server, connection closed early.'
					),
				},
				stream: new ReadableStream({
					async start(controller) {
						controller.close()
					},
				}),
			}
		}

		buffer += decoder.decode(value, { stream: true })
		const lines = buffer.split('\n')

		if (lines.length > 1) {
			// We have at least one complete line
			const firstLine = lines[0].trim()
			if (firstLine) {
				initialData = JSON.parse(firstLine, psycheJsonReviver) as ApiGetRun
				// Keep remaining partial data in buffer
				buffer = lines.slice(1).join('\n')
			} else {
				// Empty line, discard it
				buffer = lines.slice(1).join('\n')
			}
		}
	}

	// Create a stream for subsequent updates
	const stream = new ReadableStream<ApiGetRun>({
		async start(controller) {
			try {
				while (true) {
					const { value, done } = await reader.read()
					if (done) break

					buffer += decoder.decode(value, { stream: true })
					const lines = buffer.split('\n')

					// Process all complete lines except the last one (which might be incomplete)
					for (let i = 0; i < lines.length - 1; i++) {
						const line = lines[i].trim()
						if (line) {
							const data = JSON.parse(line, psycheJsonReviver) as ApiGetRun
							controller.enqueue(data)
						}
					}

					// Keep the last (potentially incomplete) line in the buffer
					buffer = lines[lines.length - 1]
				}

				// Process any remaining data in the buffer
				if (buffer.trim()) {
					const data = JSON.parse(buffer.trim(), psycheJsonReviver) as ApiGetRun
					controller.enqueue(data)
				}
			} catch (error) {
				controller.error(error)
			} finally {
				controller.close()
				// Flush the final decoder state
				decoder.decode(new Uint8Array())
			}
		},
	})

	return { initialData, stream }
}
