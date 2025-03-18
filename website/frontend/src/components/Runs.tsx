import { styled } from '@linaria/react'
import { useState } from 'react'
import { RadioSelectBar } from './RadioSelectBar.js'
import { RunSummaryCard } from './RunSummary.js'
import { RunSummary } from 'shared'
import { Sort } from './Sort.js'

const RunsContainer = styled.div`
	height: 100%;
	display: flex;
	flex-direction: column;
	position: relative;
	container-type: inline-size;
`

const RunsHeader = styled.div`
	padding: 0 24px;
	display: flex;
	flex-direction: row;
	flex-wrap: wrap;

	gap: 24px;
	padding-bottom: 24px;
	align-items: center;
	justify-content: space-between;
`

const RunBoxesContainer = styled.div`
	padding: 2px 24px;
	padding-bottom: 24px;
	display: flex;
	gap: 24px;
	display: grid;
	grid-template-columns: 1fr 1fr;
	@container (max-width: 866px) {
		grid-template-columns: 1fr;
	}
`

const runTypes = [
	{ label: 'All', value: 'all' },
	{ label: 'Active', value: 'active' },
	{
		label: 'Completed',
		value: 'completed',
	},
] as const

const runSort = [
	{ label: 'Recently updated', value: 'updated' },
	{ label: 'size', value: 'size' },
] as const

type RunType = (typeof runTypes)[number]['value']

export function Runs({ runs }: { runs: RunSummary[] }) {
	const [runTypeFilter, setRunTypeFilter] = useState<RunType>('all')
	const [sort, setSort] = useState<(typeof runSort)[number]>(runSort[0])
	return (
		<RunsContainer>
			<RunsHeader>
				<RadioSelectBar
					selected={runTypeFilter}
					options={runTypes}
					onChange={setRunTypeFilter}
				/>
				<Sort selected={sort} options={runSort} onChange={setSort} />
				{/* <Button style="secondary">train a new model</Button> */}
			</RunsHeader>
			<RunBoxesContainer>
				{runs
					.filter(
						(r) =>
							runTypeFilter === 'all' ||
							runTypeFilter === r.status.type
					)
					.map((r) => (
						<RunSummaryCard key={r.id} info={r} />
					))}
			</RunBoxesContainer>
		</RunsContainer>
	)
}
