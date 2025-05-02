import { styled } from '@linaria/react'
import { RunData, RunRoundClient, RunState, TxSummary } from 'shared'
import { forest, gold, slate } from '../colors.js'
import { text } from '../fonts.js'
import { ProgressBar } from './ProgressBar.js'
import { css } from '@linaria/core'
import { useState } from 'react'
import { useInterval } from 'usehooks-ts'
import { RunBox } from './RunBox.js'
import { c, solanaAccountUrl } from '../utils.js'
import { TxHistory } from './TxHistory.js'

const Container = styled.div`
	display: flex;
	gap: 0.5em;
	align-items: start;
	justify-content: stretch;
	& > * {
		flex-grow: 1;
		text-align: center;
	}
`

const stateNames: Record<RunState, string> = {
	Uninitialized: 'UNINITIALIZED',
	WaitingForMembers: 'WAITING FOR COMPUTE',
	Warmup: 'WARMUP',
	RoundTrain: 'TRAIN',
	RoundWitness: 'WITNESS',
	Cooldown: 'COOLDOWN',
	Finished: 'FINISHED',
	Paused: 'PAUSED',
}

const flexCol = css`
	flex-direction: column;
`

function calculateDoneRatio(
	state: Exclude<RunData['state'], undefined>,
	currentTime: Date
): number {
	const elapsedPhaseTime =
		(+currentTime -
			(state.phaseStartTime ? +state.phaseStartTime : Date.now())) /
		1000
	if (state.phase === 'WaitingForMembers') {
		return Math.min(state.clients.length / state.config.minClients, 1)
	} else if (state.phase === 'Warmup') {
		return Math.min(elapsedPhaseTime / state.config.warmupTime, 1)
	} else if (state.phase === 'Cooldown') {
		return Math.min(elapsedPhaseTime / state.config.cooldownTime, 1)
	} else if (state.phase === 'Finished') {
		return 1
	} else if (state.phase === 'Paused' || state.phase === 'Uninitialized') {
		return 0
	} else if (state.phase === 'RoundTrain') {
		const allWitnesses = state.clients.filter((c) => c.witness)
		const doneWitnesses = allWitnesses.filter((c) => c.witness === 'done')
		const witnessProgress = doneWitnesses.length / allWitnesses.length
		const timeProgress = elapsedPhaseTime / state.config.maxRoundTrainTime
		return Math.min(Math.max(witnessProgress, timeProgress), 1)
	} else if (state.phase === 'RoundWitness') {
		return Math.min(elapsedPhaseTime / state.config.roundWitnessTime, 1)
	} else {
		return 0
	}
}

const title = css`
	.theme-light & {
		background: ${slate[300]};
	}
	.theme-dark & {
		background: ${forest[600]};
	}
`

const SectionsGrid = styled.div`
	display: grid;
	grid-template-columns: repeat(4, 1fr);
	@container (width < 480px) {
		grid-template-columns: repeat(2, 1fr);
	}
	gap: 8px;
`

const waitingForMembersBox = css`
	grid-column: 1 / -1;
`

export function RunStateIndicator({
	state,
	recentTxs,
}: {
	state: Exclude<RunData['state'], undefined>
	recentTxs: TxSummary[]
}) {
	const {
		phase,
		round,
		epoch,
		config: { roundsPerEpoch, numEpochs, minClients },
		clients,
	} = state
	const [now, setNow] = useState(new Date(Date.now()))
	useInterval(() => setNow(new Date(Date.now())), 100)
	const doneRatio = calculateDoneRatio(state, now)
	return (
		<RunBox
			title={
				<span className={text['aux/xl/medium']}>
					epoch {epoch}/{numEpochs}
				</span>
			}
			titleClass={title}
		>
			<TxHistory
				txs={recentTxs.toReversed()}
				cluster={import.meta.env.VITE_COORDINATOR_CLUSTER}
			/>
			{(phase === 'Warmup' ||
				phase === 'RoundTrain' ||
				phase === 'RoundWitness' ||
				phase === 'Cooldown') && (
				<div>
					<ProgressBar
						chunkHeight={8}
						chunkWidth={4}
						chunkSpacing={1}
						ratio={
							(round +
								(phase === 'RoundWitness'
									? 0.5
									: phase === 'Cooldown'
										? 1
										: 0)) /
							roundsPerEpoch
						}
					/>
					<ProgressDescription>
						<span>round</span>
						<span>
							{round + 1}/{roundsPerEpoch}
						</span>
					</ProgressDescription>
				</div>
			)}
			{phase === 'WaitingForMembers' && (
				<div>
					<ProgressBar
						chunkHeight={8}
						chunkWidth={4}
						chunkSpacing={1}
						ratio={clients.length / minClients}
					/>
					<ProgressDescription>
						<span>compute nodes</span>
						<span>
							{clients.length}/{minClients}
						</span>
					</ProgressDescription>
				</div>
			)}
			<SectionsGrid>
				{phase === 'WaitingForMembers' ? (
					<>
						<Section
							active={phase === 'WaitingForMembers'}
							name={`${stateNames['WaitingForMembers']}`}
							className={waitingForMembersBox}
						/>
					</>
				) : (
					<>
						<Section
							active={phase === 'Warmup'}
							name={stateNames['Warmup']}
							doneRatio={doneRatio}
						/>
						<Section
							active={phase === 'RoundTrain'}
							name={stateNames['RoundTrain']}
							doneRatio={doneRatio}
						/>
						<Section
							active={phase === 'RoundWitness'}
							name={stateNames['RoundWitness']}
							doneRatio={doneRatio}
						/>

						<Section
							active={phase === 'Cooldown'}
							name={stateNames['Cooldown']}
							doneRatio={doneRatio}
						/>
					</>
				)}
			</SectionsGrid>
			<Container className={flexCol}>
				<ClientsBox>
					{clients.map((c) => (
						<RoundParticipant key={c.pubkey} client={c} />
					))}
				</ClientsBox>
			</Container>
		</RunBox>
	)
}
const ProgressDescription = styled.div`
	display: flex;
	flex-direction: row;
	gap: 8px;
	justify-content: space-between;

	.theme-dark & {
		color: ${forest[200]};
	}
`
const SectionBox = styled.div`
	display: flex;
	flex-direction: column;
	align-items: stretch;
	padding: 0.5em;
	border: 2px dashed;

	.theme-light & {
		border-color: ${slate[500]};
	}
	.theme-dark & {
		border-color: ${forest[500]};
	}

	justify-content: space-between;

	&.active {
		border-style: solid;
		background-color: ${forest[500]};
		color: ${slate[0]};
		border-color: rgba(255, 255, 255, 0.5) rgba(0, 0, 0, 0.5) rgba(0, 0, 0, 0.5)
			rgba(255, 255, 255, 0.5);
	}

	.title {
		text-align: left;
		padding-left: 0.25ch;
	}
`

const ClientBox = styled.div`
	padding: 0.5em;
	max-width: 13ch;
	aspect-ratio: 1/1;

	position: relative;

	justify-content: space-between;

	.theme-light &,
	.theme-light & a {
		color: ${slate[600]};
	}
	.theme-dark &,
	.theme-dark & a {
		color: ${forest[400]};
	}

	.invisible {
		visibility: hidden;
		margin: 1ch;
	}
	.bottom {
		position: absolute;
		bottom: 0;
		right: 0;
		width: 100%;
	}

	.right {
		position: absolute;
		right: 0;
		bottom: 0;
		height: 100%;
		writing-mode: vertical-rl;
		text-orientation: mixed;
		transform: rotate(180deg);
	}

	.top {
		position: absolute;
		top: 0;
		right: 0;
		width: 100%;
		transform: rotate(180deg);
	}

	.left {
		position: absolute;
		left: 0;
		top: 0;
		height: 100%;
		writing-mode: vertical-lr;
		text-orientation: mixed;
	}

	.link {
		position: absolute;
		top: 50%;
		right: 50%;
		width: 1em;
		height: 1em;
	}
`
const ClientsBox = styled.div`
	display: flex;
	flex-wrap: wrap;
	justify-content: space-between;
	width: 100%;
	min-height: 82px;
`

function Section({
	active,
	name,
	doneRatio,
	className,
}: {
	active: boolean
	name: string
	doneRatio?: number
	className?: string
}) {
	return (
		<SectionBox className={c(active ? 'active' : '', className)}>
			<div className="title">{name}</div>
			{!!doneRatio && (
				<ProgressBar
					ratio={active ? doneRatio : 0}
					chunkWidth={3}
					chunkSpacing={1}
					chunkHeight={1}
					size="small"
					disabled={!active}
				/>
			)}
		</SectionBox>
	)
}

function RoundParticipant({ client }: { client: RunRoundClient }) {
	const expectedPubkeyLength = 44
	const segmentLength = Math.floor(expectedPubkeyLength / 4)
	const horSegLength = segmentLength + 2
	const vertSegLength = segmentLength - 2
	return (
		<a
			href={solanaAccountUrl(
				client.pubkey,
				import.meta.env.VITE_COORDINATOR_CLUSTER
			)}
			target="_blank"
			className="link"
		>
			<ClientBox className={text['body/xs/regular']}>
				<Dot className={client.witness || 'training'} />
				<span className="invisible">
					{client.pubkey.slice(0, horSegLength)}
				</span>
				<span className="bottom">{client.pubkey.slice(0, horSegLength)}</span>
				<span className="right">
					{client.pubkey.slice(horSegLength, horSegLength + vertSegLength)}
				</span>
				<span className="top">
					{client.pubkey.slice(
						horSegLength + vertSegLength,
						horSegLength * 2 + vertSegLength
					)}
				</span>
				<span className="left">
					{client.pubkey.slice(horSegLength * 2 + vertSegLength)}
				</span>
			</ClientBox>
		</a>
	)
}

const Dot = styled.span`
	height: ${(props) => props.size ?? '3em'};
	width: ${(props) => props.size ?? '3em'};
	border-radius: 100%;
	display: inline-block;
	position: absolute;
	left: 50%;
	top: 50%;
	transform: translate(-50%, -50%);
	&.training {
		background-color: ${slate[400]};
	}
	&.waiting {
		background-color: ${gold[400]};
	}
	&.done {
		background-color: ${forest[500]};
	}
	box-shadow:
		inset -1px -1px 0px rgba(0, 0, 0, 0.5),
		inset 1px 1px 0px rgba(255, 255, 255, 0.5);
`
