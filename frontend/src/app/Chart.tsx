import { Axis } from "@visx/axis";
import { curveLinear } from "@visx/curve";
import { localPoint } from "@visx/event";
import { Grid } from "@visx/grid";
import { Group } from "@visx/group";
import { LegendOrdinal } from "@visx/legend";
import { ParentSize } from "@visx/responsive";
import { scaleLinear, scaleOrdinal, scalePower } from "@visx/scale";
import { Bar, Line, LinePath } from "@visx/shape";
import type React from "react";
import { useCallback, useLayoutEffect, useState } from "react";

interface DataPoint {
	x: number;
	y: number;
}

export interface GraphLine {
	points: DataPoint[];
	label: string;
	className: string;
	unit?: string;
}

interface LineGraphProps {
	xLabel: string;
	lines: Array<GraphLine>;
	numXMarkers?: number;
	numYMarkers?: number;
	title?: string;
	scale?: "linear" | "power";
	renderValue?: (x: number) => string;
}

const LineGraphInner: React.FC<
	LineGraphProps & { width: number; height: number }
> = ({ xLabel, renderValue, title, width, height, lines, scale }) => {
	const xMax = width;
	const yMax = height;
	const scaleKind = scale === undefined ? "linear" : scale;
	// Create scales
	const xScale = scaleLinear<number>({
		domain: [
			Math.min(...lines.flatMap(({ points }) => points.map((d) => d.x))),
			Math.max(...lines.flatMap(({ points }) => points.map((d) => d.x))) * 1.02,
		],
		range: [0, xMax],
	});

	const scaleFuncs: Record<typeof scaleKind, typeof scaleLinear> = {
		linear: scaleLinear,
		power: scalePower,
	};

	const yScale = scaleFuncs[scaleKind]<number>({
		domain: [
			Math.min(...lines.flatMap(({ points }) => points.map((d) => d.y))),
			Math.max(...lines.flatMap(({ points }) => points.map((d) => d.y))),
		],
		range: [yMax, 0],
		...(scaleKind === "power"
			? {
					exponent: 10,
				}
			: {}),
	});

	const xDomain = xScale.domain() as [number, number];
	const yDomain = yScale.domain() as [number, number];

	// Calculate marker intervals based on domain and number of markers
	const xMarkersEvery = (xDomain[1] - xDomain[0]) / 5;
	const yMarkersEvery = (yDomain[1] - yDomain[0]) / 5;

	const verticalLinePositions: number[] = [];
	for (let i = xDomain[0]; i <= xDomain[1]; i += xMarkersEvery) {
		verticalLinePositions.push(i);
	}
	const verticalGridPositions: number[] = [];
	for (let i = xDomain[0]; i <= xDomain[1]; i += xMarkersEvery / 2) {
		verticalGridPositions.push(i);
	}

	const horizontalLinePositions: number[] = [];
	for (let i = yDomain[0]; i <= yDomain[1]; i += yMarkersEvery) {
		horizontalLinePositions.push(i);
	}
	const horizontalGridPositions: number[] = [];
	for (let i = yDomain[0]; i <= yDomain[1]; i += yMarkersEvery / 2) {
		horizontalGridPositions.push(i);
	}

	const [tooltipData, setTooltipData] = useState<{
		x: number;
		y: number;
		line: GraphLine;
	} | null>(null);
	const [tooltipLeft, setTooltipLeft] = useState<number>(0);
	const [tooltipTop, setTooltipTop] = useState<number>(0);

	const handleTooltip = useCallback(
		(
			event:
				| React.TouchEvent<SVGRectElement>
				| React.MouseEvent<SVGRectElement>,
		) => {
			const { x } = localPoint(event) || { x: 0, y: 0 };
			const xValue = xScale.invert(x);

			// Find the closest point for each line
			let closestPoint: {
				point: DataPoint;
				line: GraphLine;
			} | null = null;
			let minDistance = Number.POSITIVE_INFINITY;

			for (const line of lines) {
				const point = line.points.reduce((closest, current) => {
					const distance = Math.abs(current.x - xValue);
					return distance < Math.abs(closest.x - xValue) ? current : closest;
				});

				const distance = Math.abs(point.x - xValue);
				if (distance < minDistance) {
					minDistance = distance;
					closestPoint = { point, line };
				}
			}

			if (closestPoint) {
				setTooltipData({
					x: closestPoint.point.x,
					y: closestPoint.point.y,
					line: closestPoint.line,
				});
				setTooltipLeft(xScale(closestPoint.point.x));
				setTooltipTop(yScale(closestPoint.point.y));
			}
		},
		[xScale, yScale, lines],
	);

	const handleMouseLeave = () => {
		setTooltipData(null);
	};

	return (
		<div className="relative w-full h-full">
			<svg width={width} height={height} className="absolute overflow-visible">
				<title>
					Line Graph of {lines.map((l) => l.label).join(", ")} / {xLabel}
				</title>
				<Group>
					<Grid
						width={xMax}
						height={yMax}
						xScale={xScale}
						yScale={yScale}
						rowTickValues={horizontalGridPositions}
						columnTickValues={verticalGridPositions}
						stroke="var(--color-grid)"
						strokeWidth={2}
					/>
					<Line
						from={{ x: 0, y: 0 }}
						to={{ x: xMax, y: 0 }}
						stroke="var(--color-grid)"
					/>
					<Line
						from={{ x: 0, y: yMax }}
						to={{ x: xMax, y: yMax }}
						stroke="var(--color-grid)"
					/>
					<Line
						from={{ x: 0, y: 0 }}
						to={{ x: 0, y: yMax }}
						stroke="var(--color-grid)"
					/>
					<Line
						from={{ x: xMax, y: 0 }}
						to={{ x: xMax, y: yMax }}
						stroke="var(--color-grid)"
					/>
				</Group>
				{lines.map((l) => (
					<LinePath
						key={l.label}
						data={l.points}
						x={(d) => xScale(d.x)}
						y={(d) => yScale(d.y)}
						className={l.className}
						strokeWidth={2}
						curve={curveLinear}
					/>
				))}
				<Group>
					<Axis
						orientation="bottom"
						scale={xScale}
						top={yMax}
						tickValues={verticalLinePositions}
						hideTicks
						hideAxisLine
						tickComponent={({ formattedValue, ...tickProps }) => (
							<text
								{...tickProps}
								textAnchor="middle"
								className="fill-primary font-bold text-md"
							>
								{formattedValue}
							</text>
						)}
					/>
				</Group>

				<Group>
					<Axis
						orientation="left"
						scale={yScale}
						tickValues={horizontalLinePositions}
						hideTicks
						hideAxisLine
						tickFormat={(value) =>
							renderValue
								? renderValue(value.valueOf())
								: value.valueOf().toFixed(2)
						}
						tickComponent={({ formattedValue, ...tickProps }) => {
							return (
								<text
									{...tickProps}
									textAnchor="end"
									className="fill-primary font-bold text-md"
								>
									{formattedValue}
								</text>
							);
						}}
						hideZero
					/>
				</Group>
				<Bar
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
							from={{ x: tooltipLeft, y: 0 }}
							to={{ x: tooltipLeft, y: height }}
							className="stroke-primary"
							strokeWidth={1}
							pointerEvents="none"
						/>
						<circle
							cx={tooltipLeft}
							cy={tooltipTop}
							r={4}
							className="fill-primary stroke-backdrop"
							strokeWidth={2}
							pointerEvents="none"
						/>
					</g>
				)}
			</svg>

			{tooltipData && (
				<div
					className="absolute z-10 pointer-events-none bg-primary text-backdrop text-nowrap top-8 py-1 px-2 text-center"
					style={{
						left: tooltipLeft + 10,
						top: "50%",
						transform: `translate(${
							tooltipLeft > width * (2 / 3) ? "-100%" : "0"
						}, -100%)`,
					}}
				>
					<div className="text-xl">{tooltipData.line.label}</div>
					<div>
						{xLabel} {tooltipData.x}:{" "}
						{(renderValue ?? ((x) => x.toFixed(2)))(tooltipData.y)}
						{tooltipData.line.unit ?? ""}
					</div>
				</div>
			)}
		</div>
	);
};

// Wrapper component that handles responsiveness
export const ResponsiveLineGraph: React.FC<LineGraphProps> = (props) => {
	const [forceRender, setForceRender] = useState(0);

	useLayoutEffect(() => {
		// Force a re-render after initial render with props to fix jank
		setForceRender((prev) => prev + 1);
		void props.lines; // we want to rerender if lines changes.
	}, [props.lines]);

	const title = props.title;
	const singleLine =
		props.lines.length === 1 && props.lines[0].points.length && props.lines[0];
	return (
		<div className="w-full h-full flex flex-col p-10 pt-0">
			{title && (
				<div className="text-right">
					{typeof title === "string" ? title : title[0]}
					{typeof title !== "string" && title[1]}
					{singleLine &&
						`: ${(props.renderValue ?? ((x) => x.toFixed(2)))(singleLine.points[singleLine.points.length - 1].y)}${props.lines[0].unit ?? ""}`}
				</div>
			)}
			<ParentSize key={forceRender} className="grow">
				{({ width, height }) => (
					<LineGraphInner width={width} height={height} {...props} />
				)}
			</ParentSize>
		</div>
	);
};
