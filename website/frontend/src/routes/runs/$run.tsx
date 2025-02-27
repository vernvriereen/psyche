import { createFileRoute } from '@tanstack/react-router'
import { fetchRun } from '../../fetchRuns.js'
import { Button } from '../../components/Button.js'
import ArrowLeft from '../../assets/icons/arrow-left.svg?react'
import { styled } from '@linaria/react'
import { forest, slate } from '../../colors.js'
import { text } from '../../fonts.js'
import { StatusChip } from '../../components/StatusChip.js'
import { Runtime } from '../../components/Runtime.js'
import { ProgressBar } from '../../components/ProgressBar.js'
import { MiniCard } from '../../components/MiniCard.js'
import { RadialGraph } from '../../components/RadialGraph.js'
import { formatBytes, formatNumber, metricToGraph } from '../../utils.js'
import { ResponsiveLineGraph } from '../../components/Chart.js'
import { useMemo } from 'react'
import { css } from '@linaria/core'
import { InfoChit } from '../../components/InfoChit.jsx'
export const Route = createFileRoute('/runs/$run')({
	loader: async ({ params }) => fetchRun(params.run),
	component: RouteComponent,
})

const RunContainer = styled.div`
	padding: 0 24px;
	container-type: inline-size;
	height: 100%;
`

const RunHeader = styled.div`
	display: flex;
	align-items: center;
	justify-content: space-between;
	border-bottom: 2px solid;

	.theme-light & {
		color: ${forest[700]};
		border-color: ${slate[500]};
	}
	.theme-dark & {
		color: ${forest[300]};
		border-color: ${forest[500]};
	}
`

const RunBox = styled.div`
	& > * {
		padding: 8px 16px;
	}

	margin-top: 24px;
	margin-bottom: 24px;
	border: 2px solid;

	display: flex;
	flex-direction: column;

	.theme-light & {
		border-color: ${slate[500]};
	}
	.theme-dark & {
		border-color: ${forest[500]};
	}
`

const RuntimeLabel = styled.span`
	.theme-dark & {
		color: ${forest[300]};
	}
`

const ProgressDescription = styled.div`
	display: flex;
	flex-direction: row;
	gap: 8px;
	justify-content: space-between;

	.theme-dark & {
		color: ${forest[200]};
	}
`

const StatBoxes = styled.div`
	display: flex;
	gap: 40px;
	padding: 32px;
	align-items: center;
	justify-content: center;
	flex-wrap: wrap;
`

const RadialContainer = styled.div`
	aspect-ratio: 1 / 1;
	max-height: 384px;
	height: 100cqh;
	max-width: calc(100cqw - 64px);
`

const MaybeRadialGraphContainer = styled.div`
	display: flex;
	align-items: center;
	justify-content: center;
	position: relative;
	@container (max-width: 900px) {
		flex-wrap: wrap;
	}
`

const RunContents = styled.div`
	flex-basis: 100%;
	flex-shrink: 0;
	flex-grow: 1;
	overflow-y: auto;
	display: flex;
	flex-direction: column;
	gap: 24px;
	padding: 24px;
`

const HistoryContainer = styled.div`
	display: flex;
	flex-wrap: wrap;
	gap: 24px;
	& > * {
		flex: 1 0 128px;
	}
`
const LineGraphContainer = styled.div`
	height: 128px;
	min-width: 256px;
	margin: 16px;
`

const RunDescription = styled.span`
	word-break: break-word;
`

const InfoChits = styled.div`
	display: flex;
	justify-content: space-around;
	gap: 16px;
`

function RouteComponent() {
	const {run} = Route.useLoaderData()
	const backButton = (
		<Button
			style="action"
			icon={{
				side: 'left',
				svg: ArrowLeft,
			}}
			to={'/runs'}
		>
			back
		</Button>
	)
	const graphData = useMemo(
		() => run && metricToGraph(run.metrics.history, 100),
		[run]
	)

	if (!run) {
		return (
			<RunContainer>
				{backButton}
				<RunBox>
					<RunHeader>
						<span className={text['display/4xl']}>
							run not found
						</span>
					</RunHeader>
					<div
						className={css`
							padding: 48px;
							text-align: center;
						`}
					>
						Sorry! Try another run ID.
					</div>
				</RunBox>
			</RunContainer>
		)
	}

	const info = run.info
	return (
		<RunContainer>
			<Button
				style="action"
				icon={{
					side: 'left',
					svg: ArrowLeft,
				}}
				to={'/runs'}
			>
				back
			</Button>
			<RunBox>
				<RunHeader>
					<span className={text['display/4xl']}>{info.name}</span>
					<StatusChip status={info.status.type} style="minimal">
						{info.status.type}
					</StatusChip>
				</RunHeader>

				<RunContents className={text['body/base/medium']}>
					<RunDescription>{info.description}</RunDescription>
					<InfoChits>
						<InfoChit label="params">
							{formatNumber(Number(info.size), 2)}
						</InfoChit>
						<InfoChit label="arch">{info.arch}</InfoChit>
						<InfoChit label="type">{info.type}</InfoChit>
					</InfoChits>
					<RuntimeLabel>
						runtime
						<Runtime
							start={info.startTime}
							end={
								info.status.type === 'completed'
									? info.status.at
									: undefined
							}
						/>
					</RuntimeLabel>
					<div>
						<ProgressBar
							big
							ratio={info.completedTokens / info.totalTokens}
							chunkHeight={36}
							chunkWidth={24}
						/>
						<ProgressDescription>
							<span>tokens</span>
							<span>
								{formatNumber(info.completedTokens, 3)}/
								{formatNumber(info.totalTokens, 3)}
							</span>
						</ProgressDescription>
					</div>
					<MaybeRadialGraphContainer>
						{Object.entries(run.metrics.summary.evals).length >
							3 && (
							<RadialContainer>
								<RadialGraph data={run.metrics.summary.evals} />
							</RadialContainer>
						)}
						<StatBoxes>
							
							{/* // TODO: calculate confidence and perplexity */}
							<MiniCard
								text="loss"
								value={`${run.metrics.summary.loss.toFixed(2)}`}
							/>
							<MiniCard
								text="bandwidth"
								value={`${formatBytes(run.metrics.summary.bandwidth, 2, 'bits')}ps`}
							/>
							<MiniCard
								text="training rate"
								value={`${formatNumber(run.metrics.summary.tokensPerSecond, 1, true)}tok/s`}
							/>
						</StatBoxes>
					</MaybeRadialGraphContainer>
					<HistoryContainer>
						{graphData && (
							<>
							{/* // TODO: render confidence and perplexity */}
								<LineGraphContainer>
									<ResponsiveLineGraph
										renderValue={(x) => `${x.toFixed(2)}`}
										xLabel="step"
										title="loss"
										line={{
											label: 'loss',
											points: graphData.loss,
										}}
									/>
								</LineGraphContainer>

								<LineGraphContainer>
									<ResponsiveLineGraph
										renderValue={(x) => formatNumber(x, 2)}
										xLabel="step"
										title="training speed"
										line={{
											label: 'training speed',
											points: graphData.tokensPerSecond,
											unit: ' tok/s',
										}}
									/>
								</LineGraphContainer>

								<LineGraphContainer>
									<ResponsiveLineGraph
										renderValue={(x) =>
											`${formatBytes(x, 2, 'bits')}/s`
										}
										xLabel="step"
										title="inter-node bandwidth"
										line={{
											label: 'bandwidth',
											points: graphData.bandwidth,
										}}
									/>
								</LineGraphContainer>

								{Object.entries(graphData.evals).map(
									([label, points]) => (
										<LineGraphContainer key={label}>
											<ResponsiveLineGraph
												renderValue={(x) =>
													`${(x * 100).toFixed(2)}`
												}
												xLabel="step"
												title={`Model Evaluation: ${label}`}
												line={{
													label,
													points,
													unit: '%',
												}}
											/>
										</LineGraphContainer>
									)
								)}
							</>
						)}
					</HistoryContainer>
				</RunContents>
			</RunBox>
		</RunContainer>
	)
}
