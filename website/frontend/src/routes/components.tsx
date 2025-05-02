import { createFileRoute } from '@tanstack/react-router'
import { useDarkMode } from 'usehooks-ts'
import { Button } from '../components/Button.js'
import { css } from '@linaria/core'
import { styled } from '@linaria/react'
import { RunSummaryCard } from '../components/RunSummary.js'
import { ProgressBar } from '../components/ProgressBar.js'
import Sun from '../assets/icons/sun.svg?react'
import { StatusChip } from '../components/StatusChip.js'
import { MiniCard } from '../components/MiniCard.js'
import { RadialGraph } from '../components/RadialGraph.js'
import { ResponsiveLineGraph } from '../components/Chart.js'
import { Sort } from '../components/Sort.js'
import { useState } from 'react'
import { text } from '../fonts.js'

export const Route = createFileRoute('/components')({
	component: RouteComponent,
})

const Bingus = styled.div`
	display: flex;
	gap: 8px;
	background: ${(props) => (props.dark ? 'var(--color-fg)' : 'transparent')};
	color: ${(props) => (props.dark ? 'var(--color-bg)' : 'inherit')};
	& > * {
		padding: 4px;
		border: 1px solid currentColor;
	}
	& > label > * {
		margin: 4px;
	}
`

const Section = styled.section`
	display: flex;
	flex-direction: column;
	gap: 32px;
	padding: 32px;
`

function LinkTitle({ text }: { text: string }) {
	return (
		<h2 id={text}>
			<a href={`#${text}`}>{text}</a>
		</h2>
	)
}
function RouteComponent() {
	const { isDarkMode, toggle } = useDarkMode()
	const options = [
		{
			label: 'foo',
			value: 'foo',
		},
		{ label: 'bar', value: 'bar' },
		{ label: 'baz', value: 'baz' },
		{ label: 'quux', value: 'quux' },
		{ label: 'bingus', value: 'bingus' },
	]
	const [selected, setSelected] = useState(options[0])

	return (
		<div
			className={css`
				padding: 32px;
			`}
		>
			<label
				className={css`
					position: fixed;
					background: var(--color-bg);
				`}
			>
				Dark Mode?
				<input type="checkbox" checked={isDarkMode} onChange={toggle} />
			</label>
			<h1>typography</h1>
			{Object.entries(text).map(([name, value]) => (
				<div className={value}>{name}</div>
			))}
			<h1>Components</h1>
			<Section>
				<LinkTitle text="buttons" />
				{(['primary', 'secondary', 'theme', 'action'] as const)
					.flatMap((x) =>
						(['left', 'right', null] as const).map((y) => [x, y] as const)
					)
					.map(([style, icon]) => {
						const buttonProps = {
							style,
							icon: icon
								? {
										side: icon,
										svg: Sun,
									}
								: undefined,
						}
						return (
							<div>
								<h3>
									{style} {icon ? `${icon} icon` : ''}
								</h3>
								<Bingus>
									<label>
										default
										<Button {...buttonProps}>label</Button>
									</label>
									<label>
										pressed
										<Button {...buttonProps} pressed>
											label
										</Button>
									</label>
									<label>
										disabled
										<Button {...buttonProps} disabled>
											label
										</Button>
									</label>
									<label>
										pressed & disabled
										<Button {...buttonProps} pressed disabled>
											label
										</Button>
									</label>
								</Bingus>
							</div>
						)
					})}
			</Section>
			<Section>
				<LinkTitle text="status chip" />
				{(['bold', 'minimal'] as const)
					.flatMap((x) => ([true, false] as const).map((y) => [x, y] as const))
					.map(([style, inverted]) => {
						return (
							<div>
								<h3>
									{style} {inverted ? 'inverted' : ''}
								</h3>
								<Bingus dark={inverted}>
									{(
										[
											'active',
											'funding',
											'completed',
											'waitingForMembers',
										] as const
									).map((status) => (
										<label>
											{status}
											<StatusChip
												status={status}
												style={style}
												inverted={inverted}
											>
												{status}
											</StatusChip>
										</label>
									))}
								</Bingus>
							</div>
						)
					})}
			</Section>
			<Section>
				<LinkTitle text="run" />
				<RunSummaryCard
					info={{
						id: 'run_001',
						index: 0,
						isOnlyRunAtThisIndex: true,
						name: 'land-seer',
						description: 'Processing landscape photographs',
						size: 7_000_0000n,
						totalTokens: 1000n,
						completedTokens: 750n,
						arch: 'HfLlama',
						type: 'vision',
						startTime: {
							time: new Date('2024-01-15T09:30:00'),
							slot: 12345n,
						},
						status: { type: 'active' },
						pauseHistory: [],
					}}
				/>
			</Section>
			<Section>
				<LinkTitle text="progress bar" />
				<ProgressBar chunkWidth={12} chunkHeight={24} ratio={0} />
				<ProgressBar chunkWidth={12} chunkHeight={24} ratio={0.25} />
				<ProgressBar chunkWidth={12} chunkHeight={24} ratio={0.5} />
				<ProgressBar chunkWidth={12} chunkHeight={24} ratio={0.75} />
				<ProgressBar chunkWidth={12} chunkHeight={24} ratio={1} />
			</Section>

			<Section>
				<LinkTitle text="mini card" />
				<div
					className={css`
						max-width: 512px;
						display: flex;
						flex-wrap: wrap;
						gap: 24px;
						justify-content: center;
					`}
				>
					<MiniCard text="stat stat" value="045" />
					<MiniCard text="stat stat" value="250%" />
					<MiniCard text="stat stat" value="17mtok/s" />
					<MiniCard text="stat stat" value="045" />
					<MiniCard text="stat stat" value="045" />
					<MiniCard text="stat stat" value="045" />
					<MiniCard text="stat stat" value="045" />
					<MiniCard text="stat stat" value="045" />
				</div>
			</Section>

			<Section>
				<LinkTitle text="radial graph" />
				<div
					className={css`
						height: 256px;
					`}
				>
					<RadialGraph
						data={{
							'stat stat': 0.3,
							skibidi: 0.2,
							'aim eval': 0.45,
							'ligma-5': 0.35,
							amogus: 0.56,
						}}
					/>
				</div>
			</Section>
			<Section>
				<LinkTitle text="line chart" />
				<div
					className={css`
						height: 256px;
						display: flex;
						flex-direction: row;
					`}
				>
					<ResponsiveLineGraph
						xLabel="step"
						title="eval performance"
						line={{
							label: 'eval score',
							points: [
								{
									x: 0,
									y: 3,
								},
								{
									x: 1,
									y: 5,
								},
								{
									x: 2,
									y: 1.2,
								},
								{
									x: 3,
									y: 6,
								},
							],
						}}
					/>
					<ResponsiveLineGraph
						xLabel="step"
						title="eval performance"
						line={{
							label: 'eval score',
							points: [
								{
									x: 0,
									y: 3,
								},
								{
									x: 1,
									y: 5,
								},
								{
									x: 2,
									y: 1.2,
								},
								{
									x: 3,
									y: 6,
								},
							],
						}}
					/>
					<ResponsiveLineGraph
						xLabel="step"
						title="eval performance"
						line={{
							label: 'eval score',
							points: [
								{
									x: 0,
									y: 3,
								},
								{
									x: 1,
									y: 5,
								},
								{
									x: 2,
									y: 1.2,
								},
								{
									x: 3,
									y: 6,
								},
							],
						}}
					/>
				</div>
				<div style={{ height: '256px' }}>
					<ResponsiveLineGraph
						xLabel="step"
						title="eval performance"
						line={{
							label: 'eval score',
							points: Array.from({ length: 50 }, (_, x) => ({
								x,
								y: Math.sin(x),
							})),
						}}
					/>
				</div>
			</Section>
			<Section
				className={css`
					max-width: 512px;
				`}
			>
				<LinkTitle text="sort select" />
				<Sort selected={selected} options={options} onChange={setSelected} />
			</Section>
		</div>
	)
}
