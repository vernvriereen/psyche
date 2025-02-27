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
	fakeRunData,
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

export async function fetchRun(runId: string): Promise<ApiGetRun> {
	return import.meta.env.VITE_FAKE_DATA
		? ({ run: fakeRunData[runId] } satisfies ApiGetRun)
		: psycheJsonFetch(`run/${runId}`).then((r) => r ?? null)
}

export async function fetchContributions(): Promise<ApiGetContributionInfo> {
	return import.meta.env.VITE_FAKE_DATA
		? fakeContributionInfo satisfies ApiGetContributionInfo
		: psycheJsonFetch('contributionInfo')
}
