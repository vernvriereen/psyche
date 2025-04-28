import { styled } from '@linaria/react'
import { RunData, RunRoundClient, RunState } from 'shared'
import { forest, gold, slate } from '../colors.js'
import { OutlineBox } from './OutlineBox.js'
import { c } from '../utils.js'
import { text } from '../fonts.js'
import { ProgressBar } from './ProgressBar.js'
import { css } from '@linaria/core'
import { useState } from 'react'
import { useInterval } from 'usehooks-ts'
import { Address } from './Address.js'

const Container = styled.div`
	display: flex;
	gap: 0.5em;
	align-items: start;
	justify-content: stretch;
	* {
		flex-grow: 1;
		text-align: center;
	}
`

const stateNames: Record<RunState, string> = {
	Uninitialized: 'uninitialized',
	WaitingForMembers: 'waiting for compute',
	Warmup: 'warmup',
	RoundTrain: 'train',
	RoundWitness: 'witness',
	Cooldown: 'cooldown',
	Finished: 'finished',
	Paused: 'paused',
}
const thinBorderBox = css`
	border-width: 1px;
	padding: 0.5em;
	gap: 0.5em;
`

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

export function RunStateIndicator(state: Exclude<RunData['state'], undefined>) {
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
		<OutlineBox
			title={`epoch ${epoch}/${numEpochs}`}
			titleClassName={c(text['aux/lg/medium'])}
			className={thinBorderBox}
		>
			<Container>
				{phase === 'WaitingForMembers' ? (
					<Section
						active={phase === 'WaitingForMembers'}
						name={`${stateNames['WaitingForMembers']} (${clients.length}/${minClients})`}
						doneRatio={doneRatio}
					/>
				) : (
					<>
						<Section
							active={phase === 'Warmup'}
							name={stateNames['Warmup']}
							doneRatio={doneRatio}
						/>
						<Container>
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
						</Container>

						<Section
							active={phase === 'Cooldown'}
							name={stateNames['Cooldown']}
							doneRatio={doneRatio}
						/>
					</>
				)}
			</Container>
			<Container className={flexCol}>
				<ClientsBox>
					{clients.map((c) => (
						<RoundParticipant key={c.pubkey} client={c} />
					))}
				</ClientsBox>
			</Container>
			{(phase === 'RoundTrain' ||
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
		</OutlineBox>
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
	border: 1px dashed;

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
		box-shadow:
			inset -1px -1px 0px rgba(0, 0, 0, 0.5),
			inset 1px 1px 0px rgba(255, 255, 255, 0.5);
	}
`

const ClientBox = styled.div`
	display: flex;
	flex-direction: column;
	align-items: center;
	margin: 0.5em;
	padding: 0.5em;
	max-width: 12ch;

	justify-content: space-between;

	.theme-light &,
	.theme-light & a {
		color: ${forest[500]};
	}
	.theme-dark &,
	.theme-dark & a {
		color: ${forest[300]};
	}
`
const ClientsBox = styled.div`
	display: flex;
	flex-wrap: wrap;
`

function Section({
	active,
	name,
	doneRatio,
}: {
	active: boolean
	name: string
	doneRatio: number
}) {
	return (
		<SectionBox className={active ? 'active' : ''}>
			{name}
			<ProgressBar
				ratio={active ? doneRatio : 0}
				chunkWidth={3}
				chunkSpacing={1}
				chunkHeight={1}
				size="small"
				disabled={!active}
			/>
		</SectionBox>
	)
}

function RoundParticipant({ client }: { client: RunRoundClient }) {
	return (
		<ClientBox className={text['aux/xs/regular']}>
			<Dot className={client.witness} />
			<Address address={client.pubkey} copy={false} />
		</ClientBox>
	)
}

const Dot = styled.span`
	margin: 0.5em;
	height: ${(props) => props.size ?? '2em'};
	width: ${(props) => props.size ?? '2em'};
	border-radius: 100%;
	display: inline-block;
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
// const Legend = styled.div`
// 	display: flex;
// 	gap: 1em;
// 	div {
// 		display: flex;
// 		align-items: center;
// 	}
// `
