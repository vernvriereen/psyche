import { createFileRoute } from '@tanstack/react-router'
import { Runs } from '../../components/Runs.js'
import { fetchRuns } from '../../fetchRuns.js'

export const Route = createFileRoute('/runs/')({
	loader: fetchRuns,
	component: RouteComponent,
})

function RouteComponent() {
	const runs = Route.useLoaderData()
	return <Runs runs={runs.runs} />
}
