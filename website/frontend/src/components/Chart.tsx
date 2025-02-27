import { Axis } from '@visx/axis'
import { localPoint } from '@visx/event'
import { Group } from '@visx/group'
import { ParentSize } from '@visx/responsive'
import { scaleLinear, scalePower } from '@visx/scale'
import { Bar, Line, LinePath } from '@visx/shape'
import type React from 'react'
import { useCallback, useState } from 'react'
import { GridOfPlusSymbols } from './ChartPlus.js'
import { useDarkMode } from 'usehooks-ts'
import { forest, lime, slate } from '../colors.js'
import { styled } from '@linaria/react'
import { text } from '../fonts.js'

interface DataPoint {
	x: number
	y: number
}

export interface GraphLine {
	points: DataPoint[]
	label: string
	unit?: string
}

interface LineGraphProps {
	xLabel: string
	line: GraphLine
	numXMarkers?: number
	numYMarkers?: number
	title?: string
	scale?: 'linear' | 'power'
	renderValue?: (x: number) => string
}

const margin = {
	right: 16,
	left: 24,
	bottom: 24,
}

const padding = {
	left: 8,
	bottom: 8,
}

const Tooltip = styled.div`
	position: absolute;
	z-index: 10;
	pointer-events: none;
	white-space: nowrap;
	padding: 2px 4px;
	text-align: center;
	background: ${forest[700]};
	color: ${slate[0]};
	overflow: visible;
`

const GraphContainer = styled.div`
	position: relative;
	height: 100%;
`

const Title = styled.div`
	color: ${(props) => props.color};
	pointer-events: none;
	text-transform: uppercase;
`

const WaitingForData = styled.div`
	display: flex;
	align-items: center;
	justify-content: center;
	height: 100%;
`

function integerMarkers(
	domain: [number, number],
	targetMarkers: number
): number[] {
	const [min, max] = domain
	const range = max - min

	// ideal spacing if we didn't care about hitting ints
	const idealSpacing = range / targetMarkers

	// find the closest "nice" int step size
	const potentialSteps = [1, 2, 5, 10, 20, 25, 50, 100, 200, 500, 1000, 2000, 5000, 10_000, 20_000, 50_000]
	const step = potentialSteps.reduce((prev, curr) => {
		return Math.abs(curr - idealSpacing) < Math.abs(prev - idealSpacing)
			? curr
			: prev
	})

	const idealMarkers: number[] = []
	const start = Math.ceil(min)
	const end = Math.ceil(max)

	for (let i = start; i < end; i += step) {
		idealMarkers.push(i)
	}
	if (idealMarkers.at(-1) !== max) {
		idealMarkers.pop()
		idealMarkers.push(max)
	}

	return idealMarkers
}

function interpolateArray(arr: number[], n: number) {
	if (arr.length < 2) return arr

	const result = []

	for (let i = 0; i < arr.length - 1; i++) {
		const start = arr[i]
		const end = arr[i + 1]
		const step = (end - start) / (n + 1)

		result.push(start)

		for (let j = 1; j <= n; j++) {
			result.push(start + step * j)
		}
	}

	result.push(arr[arr.length - 1])

	return result
}

// overcomplicated logic to make sure min & max are never both the same OR 0,
// lest we infinite loop trying to do tick markets.
function scaleDomainToEnsureSpace(nums: number[]): [number, number] {
	const minDomain = Math.min(...nums)
	const maxDomain = Math.max(...nums)
	return [
		minDomain,
		maxDomain === minDomain
			? minDomain === 0
				? 1
				: maxDomain * 1.1
			: maxDomain,
	]
}

const LineGraphInner: React.FC<
	LineGraphProps & { width: number; height: number }
> = ({ xLabel, renderValue, title: rawTitle, width, height, line, scale }) => {
	const xMax = width - margin.left - margin.right - padding.left
	const yMax = height - margin.bottom - padding.bottom

	const scaleKind = scale === undefined ? 'linear' : scale

	const xScale = scaleLinear<number>({
		domain: scaleDomainToEnsureSpace(line.points.map((d) => d.x)),
		range: [0, xMax],
	})

	const scaleFuncs: Record<typeof scaleKind, typeof scaleLinear> = {
		linear: scaleLinear,
		power: scalePower,
	}

	const yScale = scaleFuncs[scaleKind]<number>({
		domain: scaleDomainToEnsureSpace(line.points.map((d) => d.y)),
		range: [yMax, 0],
		...(scaleKind === 'power'
			? {
					exponent: 10,
				}
			: {}),
	})

	const [tooltipData, setTooltipData] = useState<{
		x: number
		y: number
		posX: number
		top: number
	} | null>(null)

	const handleTooltip = useCallback(
		(
			event:
				| React.TouchEvent<SVGRectElement>
				| React.MouseEvent<SVGRectElement>
		) => {
			const { x } = localPoint(event) || { x: 0, y: 0 }
			const xValue = xScale.invert(x - margin.left - padding.left)

			// find the closest point on the line
			let closestPoint: DataPoint | null = null
			let minDistance = Number.POSITIVE_INFINITY

			const point = line.points.reduce((closest, current) => {
				const distance = Math.abs(current.x - xValue)
				return distance < Math.abs(closest.x - xValue)
					? current
					: closest
			})

			const distance = Math.abs(point.x - xValue)
			if (distance < minDistance) {
				minDistance = distance
				closestPoint = point
			}

			if (closestPoint) {
				setTooltipData({
					x: closestPoint.x,
					y: closestPoint.y,
					posX: xScale(closestPoint.x) + margin.left + padding.left,
					top: yScale(closestPoint.y) + padding.bottom,
				})
			}
		},
		[xScale, line, yScale]
	)

	const handleMouseLeave = useCallback(() => {
		setTooltipData(null)
	}, [setTooltipData])

	const { isDarkMode } = useDarkMode()

	const title = rawTitle ?? `Line Graph of ${line.label} / ${xLabel}`

	const lineColor = forest[isDarkMode ? 300 : 600]
	const plusColor = slate[isDarkMode ? 500 : 300]
	const verticalGridColor = slate[isDarkMode ? 600 : 500]
	const labelColor = isDarkMode ? forest[300] : slate[1000]

	if (line.points.length < 2) {
		return (
			<GraphContainer>
				{title && (
					<Title
						color={labelColor}
						className={text['body/sm/semibold']}
					>
						{title}
					</Title>
				)}
				<WaitingForData>waiting for data...</WaitingForData>
			</GraphContainer>
		)
	}

	const xDomain = xScale.domain() as [number, number]
	const yDomain = yScale.domain() as [number, number]

	const numXMarkers = width / 96
	const numYMarkers = height / 32

	// Calculate marker intervals based on domain and number of markers
	const verticalLinePositions = integerMarkers(
		[xDomain[0], xDomain[1]],
		numXMarkers
	)

	const xAxisLabelPositions = interpolateArray(verticalLinePositions, 2)

	// Horizontal lines
	const yMarkersEvery = (yDomain[1] - yDomain[0]) / numYMarkers

	const horizontalLinePositions: number[] = []
	for (let i = yDomain[0]; i <= yDomain[1]; i += yMarkersEvery) {
		horizontalLinePositions.push(i)
	}

	// Major ticks Y-axis
	const majorTicksPerHorizontalLine = 2
	const majorTickValuesYAxis: number[] = []
	for (
		let i = yDomain[0];
		i <= yDomain[1];
		i += yMarkersEvery / majorTicksPerHorizontalLine
	) {
		majorTickValuesYAxis.push(i)
	}

	// Minor ticks Y-axis
	const minorTicksPerMajorTickY = 5
	const allTickValuesYAxis: number[] = []
	for (
		let i = yDomain[0];
		i <= yDomain[1];
		i +=
			yMarkersEvery /
			(majorTicksPerHorizontalLine * minorTicksPerMajorTickY)
	) {
		allTickValuesYAxis.push(i)
	}

	const centerLineThickness = 1
	const plusSize = 6
	const plusThickness = 1

	return (
		<GraphContainer>
			{title && (
				<Title color={labelColor} className={text['body/sm/semibold']}>
					{title}
				</Title>
			)}
			<svg width={width} height={height} style={{ position: 'absolute' }}>
				<title>{title}</title>
				<Group left={margin.left + padding.left} top={padding.bottom}>
					{verticalLinePositions.map((x, i) => (
						<line
							key={i}
							stroke={verticalGridColor}
							x1={xScale(x)}
							y1={-plusSize}
							x2={xScale(x)}
							y2={yMax + plusSize}
							strokeWidth={centerLineThickness}
						/>
					))}

					<GridOfPlusSymbols
						stroke={plusColor}
						splitStroke={verticalGridColor}
						horizontalLinePositions={horizontalLinePositions}
						verticalLinePositions={verticalLinePositions}
						xScale={xScale}
						yScale={yScale}
						size={plusSize}
						thickness={plusThickness}
						centerLineThickness={0}
					/>
					<LinePath
						key={line.label}
						data={line.points}
						x={(d) => xScale(d.x)}
						y={(d) => yScale(d.y)}
						strokeWidth={3}
						stroke={lineColor}
						strokeLinecap="butt"
					/>
				</Group>

				<Group top={padding.bottom} left={margin.left + padding.left}>
					<Axis
						orientation="bottom"
						scale={xScale}
						top={yMax}
						tickValues={xAxisLabelPositions}
						hideTicks
						hideAxisLine
						tickFormat={(value, index) =>
							index % 3 === 0
								? `${+value.valueOf().toFixed(2)}`
								: ''
						}
						tickComponent={({ formattedValue, ...tickProps }) =>
							formattedValue && (
								<text {...tickProps} fill={labelColor}>
									<tspan>{formattedValue}</tspan>
								</text>
							)
						}
					/>
				</Group>
				<Group left={margin.left} top={padding.bottom}>
					<Axis
						orientation="left"
						scale={yScale}
						tickValues={majorTickValuesYAxis}
						hideAxisLine
						hideTicks
						tickFormat={(value, index) =>
							index % 2 == 0
								? `${value.valueOf() >= 0 ? '' : '-'}${(renderValue ?? ((x) => x.toFixed(1)))(value.valueOf()).slice(0, 7)}`
								: ''
						}
						tickComponent={({ formattedValue, ...tickProps }) => {
							return (
								formattedValue && (
									<text
										{...tickProps}
										dy={'0.6ch'}
										fill={labelColor}
									>
										{formattedValue}
									</text>
								)
							)
						}}
						hideZero
					/>
				</Group>
				<Bar
					x={margin.left + padding.left}
					y={padding.bottom}
					width={width}
					height={height}
					fill="transparent"
					onTouchStart={handleTooltip}
					onTouchMove={handleTooltip}
					onMouseMove={handleTooltip}
					onMouseLeave={handleMouseLeave}
				/>
				{tooltipData && (
					<g>
						<Line
							from={{ x: tooltipData.posX, y: padding.bottom }}
							to={{
								x: tooltipData.posX,
								y: height - margin.bottom,
							}}
							stroke={lineColor}
							strokeWidth={1}
							pointerEvents="none"
						/>
						<circle
							cx={tooltipData.posX}
							cy={tooltipData.top}
							r={4}
							fill={lime[500]}
							pointerEvents="none"
						/>
					</g>
				)}
			</svg>

			{tooltipData && (
				<Tooltip
					className={text['button/sm']}
					style={{
						top: '50%',
						transform: 'translate(0, -100%)',
						...(tooltipData.posX > width / 2
							? {
									right: width - (tooltipData.posX - 10),
								}
							: {
									left: tooltipData.posX + 10,
								}),
					}}
				>
					<div className="font-eva text-xl">{line.label}</div>
					<div>
						{xLabel} {tooltipData.x}:{' '}
						{(renderValue ?? ((x) => x.toFixed(2)))(tooltipData.y)}
						{line.unit ?? ''}
					</div>
				</Tooltip>
			)}
		</GraphContainer>
	)
}

export const ResponsiveLineGraph: React.FC<LineGraphProps> = (props) => {
	return (
		<ParentSize debounceTime={10}>
			{({ width, height }) => (
				<LineGraphInner width={width} height={height} {...props} />
			)}
		</ParentSize>
	)
}
