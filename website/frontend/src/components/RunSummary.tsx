import { styled } from '@linaria/react'
import { StatusChip } from './StatusChip.js'
import { text } from '../fonts.js'
import { InfoChit } from './InfoChit.js'
import { Runtime } from './Runtime.js'
import { formatNumber } from '../utils.js'
import { RunSummary } from 'shared'
import { ShadowCard } from './ShadowCard.jsx'
import { forest, slate } from '../colors.js'
import { useMemo } from 'react'
import { Progress } from './ProgressWrapper.js'

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
		pauseHistory,
	},
}: {
	info: RunSummary
}) {
	const pauses = useMemo(
		() => pauseHistory.map((p) => [p[0], p[1].time] as const),
		[pauseHistory]
	)
	return (
		<ShadowCard
			to="/runs/$run"
			params={{
				run: id,
			}}
		>
			<RunHeader>
				<RunTitleRow>
					<RunTitle className={text['display/2xl']}>{name || id}</RunTitle>
					<StatusChip status={status.type} style="minimal" />
				</RunTitleRow>
				<RunDescription className={text['aux/xs/regular']}>
					{description}
				</RunDescription>
			</RunHeader>

			<InfoChits>
				{size !== 0n && (
					<InfoChit label="params">{formatNumber(Number(size), 2)}</InfoChit>
				)}
				<InfoChit label="arch">{arch}</InfoChit>
				<InfoChit label="type">{type}</InfoChit>
				<InfoChit label="tokens">
					{formatNumber(Number(totalTokens), 2)}
				</InfoChit>
			</InfoChits>

			<Progress
				label="tokens"
				chunkHeight={12}
				chunkWidth={16}
				current={Number(completedTokens)}
				total={Number(totalTokens)}
			/>
			<div className={text['aux/xs/regular']}>
				runtime{' '}
				<Runtime
					start={startTime.time}
					pauses={pauses}
					end={status.type === 'completed' ? status.at.time : undefined}
				/>
			</div>
		</ShadowCard>
	)
}
