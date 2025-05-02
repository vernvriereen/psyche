import { styled } from '@linaria/react'
import { useState } from 'react'
import { RadioSelectBar } from './RadioSelectBar.js'
import { RunSummaryCard } from './RunSummary.js'
import { ApiGetRuns } from 'shared'
import { Sort } from './Sort.js'
import { text } from '../fonts.js'

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
	padding-bottom: 1em;
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

const GlobalStats = styled.div`
	padding: 1em 24px;
	display: flex;
	flex-wrap: wrap;
	gap: 1em;
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

export function Runs({
	runs,
	totalTokens,
	totalTokensPerSecondActive,
}: ApiGetRuns) {
	const [runTypeFilter, setRunTypeFilter] = useState<RunType>('all')
	const [sort, setSort] = useState<(typeof runSort)[number]>(runSort[0])
	return (
		<RunsContainer>
			<GlobalStats>
				<GlobalStat
					label="tokens/sec"
					value={totalTokensPerSecondActive.toLocaleString()}
				/>
				<GlobalStat
					label="tokens trained"
					value={totalTokens.toLocaleString()}
				/>
			</GlobalStats>
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
						(r) => runTypeFilter === 'all' || runTypeFilter === r.status.type
					)
					.map((r) => (
						<RunSummaryCard key={r.id} info={r} />
					))}
			</RunBoxesContainer>
		</RunsContainer>
	)
}

const StatBox = styled.span`
	border: 2px solid currentColor;
	display: inline-flex;
	gap: 0.5em;
	align-items: center;
	padding: 0.5em;
`

function GlobalStat({ label, value }: { value: string; label: string }) {
	return (
		<StatBox>
			<span className={text['display/2xl']}>{value}</span>
			<span className={text['body/sm/regular']}>{label}</span>
		</StatBox>
	)
}
