import { styled } from '@linaria/react'
import { RunData, RunRoundClient, RunState } from 'shared'
import { forest, gold, slate } from '../colors.js'
import { OutlineBox } from './OutlineBox.js'
import { c } from '../utils.js'
import { text } from '../fonts.js'
import { ProgressBar } from './ProgressBar.js'
import { css } from '@linaria/core'

const Container = styled.div`
	display: flex;
	gap: 0.5em;
	align-items: start;
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

const bottomBorderHold = css`
padding-bottom: 1em	
border-bottom: 1px solid black;
`

export function RunStateIndicator({
	phase,
	round,
	epoch,
	roundsPerEpoch,
	numEpochs,
	clients,
}: Exclude<RunData['state'], undefined>) {
	return (
		<OutlineBox
			title={`epoch ${epoch}/${numEpochs}`}
			titleClassName={c(text['aux/lg/medium'])}
			className={thinBorderBox}
		>
			<Container>
				<Section
					active={phase === 'WaitingForMembers'}
					name={stateNames['WaitingForMembers']}
				/>
				<Section active={phase === 'Warmup'} name={stateNames['Warmup']} />
				<Container className={bottomBorderHold}>
					<Section
						active={phase === 'RoundTrain'}
						name={stateNames['RoundTrain']}
					/>
					<Section
						active={phase === 'RoundWitness'}
						name={stateNames['RoundWitness']}
					/>
				</Container>

				<Section active={phase === 'Cooldown'} name={stateNames['Cooldown']} />
			</Container>
			<Container className={flexCol}>
				<Legend>
					<div>
						<Dot size="1em" />
						trainer
					</div>
					<div>
						<Dot size="1em" className="waiting" />
						unfinished witness
					</div>
					<div>
						<Dot size="1em" className="done" />
						finished witness
					</div>
				</Legend>
				<ClientsBox>
					{clients.map((c) => (
						<RoundParticipant key={c.pubkey} client={c} />
					))}
				</ClientsBox>
			</Container>
			<div>
				<ProgressBar
					chunkHeight={8}
					chunkWidth={4}
					chunkSpacing={1}
					ratio={round / roundsPerEpoch}
				/>
				<ProgressDescription>
					<span>round</span>
					<span>
						{round+1}/{roundsPerEpoch}
					</span>
				</ProgressDescription>
			</div>
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
	align-items: center;
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

	justify-content: space-between;
`
const ClientsBox = styled.div`
	display: flex;
	flex-wrap: wrap;
`

function Section({ active, name }: { active: boolean; name: string }) {
	return <SectionBox className={active ? 'active' : ''}>{name}</SectionBox>
}

function RoundParticipant({ client }: { client: RunRoundClient }) {
	console.log(client)
	return (
		<ClientBox>
			<Dot className={client.witness} />
			{client.pubkey.slice(0, 6)}
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
const Legend = styled.div`
	display: flex;
	gap: 1em;
	div {
		display: flex;
		align-items: center;
	}
`
