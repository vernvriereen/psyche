import { styled } from '@linaria/react'
import { StatusChip } from './StatusChip.js'
import { text } from '../fonts.js'
import { InfoChit } from './InfoChit.js'
import { ProgressBar } from './ProgressBar.js'
import { Runtime } from './Runtime.js'
import { formatNumber } from '../utils.js'
import { RunSummary } from 'shared'
import { ShadowCard } from './ShadowCard.jsx'
import { forest, slate } from '../colors.js'

const RunTitleRow = styled.div`
	display: flex;
	flex-direction: row;
	align-items: flex-start;
	justify-content: space-between;
	gap: 8px;
`

const RunHeader = styled.div`
	width: 100%;
	display: flex;
	flex-direction: column;
	gap: 4px;
`

const RunTitle = styled.span`
	color: var(--color-fg);
	overflow: hidden;
	text-overflow: ellipsis;
	display: -webkit-box;
	-webkit-line-clamp: 2;
	-webkit-box-orient: vertical;
`

const RunDescription = styled.span`
	word-wrap: break-word;
	overflow: hidden;
	text-overflow: ellipsis;
	display: -webkit-box;
	-webkit-line-clamp: 2;
	-webkit-box-orient: vertical;
	.theme-light & {
		color: ${forest[700]};
	}
	.theme-dark & {
		color: ${slate[0]};
	}
`
const InfoChits = styled.div`
	display: flex;
	gap: 16px;
`

const Progress = styled.div`
	display: flex;
	flex-direction: column;
	gap: 8px;
`

const ProgressDescription = styled.div`
	display: flex;
	flex-direction: row;
	gap: 8px;
	justify-content: space-between;
`

export function RunSummaryCard({
	info: {
		id,
		arch,
		completedTokens,
		description,
		name,
		size,
		startTime,
		totalTokens,
		status,
		type,
	},
}: {
	info: RunSummary
}) {
	return (
		<ShadowCard
			to="/runs/$run"
			params={{
				run: id,
			}}
		>
			<RunHeader>
				<RunTitleRow>
					<RunTitle className={text['display/2xl']}>{name}</RunTitle>
					<StatusChip status={status.type} style="minimal">
						{status.type}
					</StatusChip>
				</RunTitleRow>
				<RunDescription className={text['aux/xs/regular']}>
					{description}
				</RunDescription>
			</RunHeader>

			<InfoChits>
				<InfoChit label="params">
					{formatNumber(Number(size), 2)}
				</InfoChit>
				<InfoChit label="arch">{arch}</InfoChit>
				<InfoChit label="type">{type}</InfoChit>
				<InfoChit label="tokens">
					{formatNumber(totalTokens, 2)}
				</InfoChit>
			</InfoChits>

			<Progress>
				<ProgressBar
					ratio={completedTokens / totalTokens}
					chunkHeight={16}
					chunkWidth={12}
				/>
				<ProgressDescription className={text['aux/xs/regular']}>
					<span>tokens</span>
					<span>
						{formatNumber(completedTokens, 3)}/
						{formatNumber(totalTokens, 3)}
					</span>
				</ProgressDescription>
			</Progress>
			<div className={text['aux/xs/regular']}>
				runtime{' '}
				<Runtime
					start={startTime}
					end={status.type === 'completed' ? status.at : undefined}
				/>
			</div>
		</ShadowCard>
	)
}
