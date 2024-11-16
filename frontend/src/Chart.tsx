import { Axis } from "@visx/axis";
import { curveLinear } from "@visx/curve";
import { localPoint } from "@visx/event";
import { Group } from "@visx/group";
import { LegendOrdinal } from "@visx/legend";
import { ParentSize } from "@visx/responsive";
import { scaleLinear, scaleOrdinal, scalePower } from "@visx/scale";
import { Bar, Line, LinePath } from "@visx/shape";
import type React from "react";
import { useCallback, useLayoutEffect, useState } from "react";
import { GridOfPlusSymbols } from "./ChartPlus";
import { TextStretcher } from "./TextStretcher";
import { twStrokeToColor } from "./palette";
import useTailwind from "./tailwind";

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
  title?: string | [string, string];
  scale?: "linear" | "power";
  renderValue?: (x: number) => string;
}

const LineGraphInner: React.FC<LineGraphProps & { width: number; height: number }> = ({
  xLabel,
  renderValue,
  title,
  width,
  height,
  lines,
  numXMarkers = 4,
  numYMarkers = 8,
  scale,
}) => {
  // Calculate bounds
  const margin = {
    right: 0,
    left: 80,
    bottom: 60,
  };

  const padding = {
    left: 8,
    bottom: 8,
  };
  const xMax = width - margin.left - margin.right - padding.left;
  const yMax = height - margin.bottom - padding.bottom;

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
  const xMarkersEvery = (xDomain[1] - xDomain[0]) / (numXMarkers || 10);
  const yMarkersEvery = (yDomain[1] - yDomain[0]) / (numYMarkers || 10);

  // Vertical lines
  const verticalLinePositions: number[] = [];
  for (let i = xDomain[0]; i <= xDomain[1]; i += xMarkersEvery) {
    verticalLinePositions.push(i);
  }

  // Major ticks X-axis
  const majorTicksPerVerticalLine = 6;
  const majorTickValuesXAxis: number[] = [];
  for (let i = xDomain[0]; i <= xDomain[1]; i += xMarkersEvery / majorTicksPerVerticalLine) {
    majorTickValuesXAxis.push(i);
  }

  // Minor ticks X-axis
  const minorTicksPerMajorTick = 5;
  const allTickValuesXAxis: number[] = [];
  for (let i = xDomain[0]; i <= xDomain[1]; i += xMarkersEvery / (majorTicksPerVerticalLine * minorTicksPerMajorTick)) {
    allTickValuesXAxis.push(i);
  }

  // Horizontal lines
  const horizontalLinePositions: number[] = [];
  for (let i = yDomain[0]; i <= yDomain[1]; i += yMarkersEvery) {
    horizontalLinePositions.push(i);
  }

  // Major ticks Y-axis
  const majorTicksPerHorizontalLine = 2;
  const majorTickValuesYAxis: number[] = [];
  for (let i = yDomain[0]; i <= yDomain[1]; i += yMarkersEvery / majorTicksPerHorizontalLine) {
    majorTickValuesYAxis.push(i);
  }

  // Minor ticks Y-axis
  const minorTicksPerMajorTickY = 5;
  const allTickValuesYAxis: number[] = [];
  for (
    let i = yDomain[0];
    i <= yDomain[1];
    i += yMarkersEvery / (majorTicksPerHorizontalLine * minorTicksPerMajorTickY)
  ) {
    allTickValuesYAxis.push(i);
  }

  const centerLineThickness = 3;

  const plusSize = 8;
  const plusThickness = 2;

  const tw = useTailwind();

  // Create scale for the legend
  const ordinalScale = scaleOrdinal({
    domain: lines.map((line) => {
      return `${line.label}: ${(renderValue ?? ((x) => x.toFixed(3)))(line.points.at(-1)?.y ?? 0)}${line.unit ?? ""}`;
    }),
    range: lines.map((line) => twStrokeToColor(line.className, tw)),
  });

  const singleLine = lines.length === 1 && lines[0].points.length && lines[0];

  const [tooltipData, setTooltipData] = useState<{
    x: number;
    y: number;
    line: GraphLine;
  } | null>(null);
  const [tooltipLeft, setTooltipLeft] = useState<number>(0);
  const [tooltipTop, setTooltipTop] = useState<number>(0);

  const handleTooltip = useCallback(
    (event: React.TouchEvent<SVGRectElement> | React.MouseEvent<SVGRectElement>) => {
      const { x } = localPoint(event) || { x: 0, y: 0 };
      const xValue = xScale.invert(x - margin.left - padding.left);

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
        setTooltipLeft(xScale(closestPoint.point.x) + margin.left + padding.left);
        setTooltipTop(yScale(closestPoint.point.y) + padding.bottom);
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
        <Group left={margin.left + padding.left} top={padding.bottom}>
          {verticalLinePositions.map((x, i) => (
            <line
              // biome-ignore lint/suspicious/noArrayIndexKey: these are actually only indexed by key, this is safe.
              key={i}
              className="stroke-grid"
              x1={xScale(x)}
              y1={-plusSize}
              x2={xScale(x)}
              y2={yMax + plusSize}
              strokeWidth={centerLineThickness}
            />
          ))}

          <GridOfPlusSymbols
            horizontalLinePositions={horizontalLinePositions}
            verticalLinePositions={verticalLinePositions}
            xScale={xScale}
            yScale={yScale}
            size={plusSize}
            thickness={plusThickness}
            centerLineThickness={centerLineThickness * 1.5}
          />
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
        </Group>

        <Line
          from={{ x: 0, y: height - margin.bottom + padding.bottom }}
          to={{ x: width, y: height - margin.bottom + padding.bottom }}
          className="stroke-primary"
          strokeWidth={4}
        />
        <Group top={10 + padding.bottom} left={margin.left + padding.left}>
          <Axis
            orientation="bottom"
            scale={xScale}
            top={yMax}
            tickValues={allTickValuesXAxis}
            tickLength={6}
            tickStroke="currentColor"
            tickTransform="translate(0,1)"
            hideAxisLine
            tickComponent={() => null}
          />
          <Axis
            orientation="bottom"
            scale={xScale}
            top={yMax}
            tickValues={majorTickValuesXAxis}
            tickLength={12}
            strokeWidth={2}
            tickStroke="currentColor"
            tickTransform="translate(0,2)"
            hideAxisLine
            tickFormat={(value) =>
              majorTickValuesXAxis.indexOf(value.valueOf()) % 2 === 0
                ? `${value.valueOf() === 0 ? "" : value.valueOf() >= 0 ? "+" : "-"}${+value.valueOf().toFixed(0)}`
                : ""
            }
            tickComponent={({ formattedValue, ...tickProps }) =>
              formattedValue && (
                <text
                  {...tickProps}
                  fontSize={formattedValue === "0" ? "2em" : "1.2em"}
                  fontWeight="bold"
                  textAnchor="middle"
                  dy="1ch"
                  className="fill-primary md"
                >
                  <tspan dx={formattedValue === "0" ? "0" : "-0.5ch"}>{formattedValue.slice(0, 1)}</tspan>
                  <tspan>{formattedValue.slice(1)}</tspan>
                </text>
              )
            }
          />
        </Group>

        <Line
          from={{ x: padding.left * 1.5, y: 0 }}
          to={{
            x: padding.left * 1.5,
            y: height - margin.bottom - padding.bottom * 3,
          }}
          className="stroke-primary"
          strokeWidth={4}
        />
        <Group left={margin.left} top={padding.bottom}>
          <Axis
            orientation="left"
            scale={yScale}
            tickValues={allTickValuesYAxis}
            tickLength={6}
            tickStroke="currentColor"
            tickTransform="translate(-1,0)"
            hideAxisLine
            tickComponent={() => null}
          />
          <Axis
            orientation="left"
            scale={yScale}
            tickValues={majorTickValuesYAxis}
            tickLength={12}
            strokeWidth={2}
            tickStroke="currentColor"
            tickTransform="translate(-1,0)"
            hideAxisLine
            tickFormat={(value) =>
              majorTickValuesYAxis.includes(value.valueOf())
                ? `${value.valueOf() >= 0 ? "" : "-"}${(renderValue ?? ((x) => x.toFixed(1)))(value.valueOf()).slice(0, 4)}`
                : ""
            }
            tickComponent={({ formattedValue, ...tickProps }) => {
              const isAtBottom = tickProps.y === yMax;
              return (
                formattedValue && (
                  <Group>
                    {!isAtBottom && (
                      <>
                        <Line
                          from={{ x: tickProps.x - 47, y: tickProps.y }}
                          to={{ x: tickProps.x - 51, y: tickProps.y }}
                          className="stroke-primary"
                          strokeWidth={2}
                        />

                        <Line
                          from={{ x: tickProps.x - 59, y: tickProps.y }}
                          to={{ x: tickProps.x - 63, y: tickProps.y }}
                          className="stroke-primary"
                          strokeWidth={2}
                        />
                      </>
                    )}
                    <text
                      {...tickProps}
                      fontSize={isAtBottom ? 24 : 20}
                      fontWeight="bold"
                      textAnchor="end"
                      dy={isAtBottom ? "0.2ch" : "0.6ch"}
                      dx="-0.5ch"
                      className="fill-primary md"
                    >
                      {formattedValue}
                    </text>
                  </Group>
                )
              );
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
              from={{ x: tooltipLeft, y: padding.bottom }}
              to={{ x: tooltipLeft, y: height - margin.bottom }}
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
            transform: "translate(0, -100%)",
          }}
        >
          <div className="font-eva text-xl">{tooltipData.line.label}</div>
          <div>
            {xLabel} {tooltipData.x}: {(renderValue ?? ((x) => x.toFixed(2)))(tooltipData.y)}
            {tooltipData.line.unit ?? ""}
          </div>
        </div>
      )}
      <div
        className="absolute top-0 bg-backdrop/70 pointer-events-none"
        style={{ left: `${padding.left + margin.left + 12}px` }}
      >
        {title && (
          <>
            <div className="xl:w-32 md:w-24 w-16">
              <TextStretcher className="p-2 xl:h-12 md:h-10 h-8 border-2 rounded-md border-primary">
                {typeof title === "string" ? title : title[0]}
              </TextStretcher>
              {typeof title !== "string" && (
                <TextStretcher className="xl:my-2 my-1 xl:h-12 md:h-4 h-3">{title[1]}</TextStretcher>
              )}
              {singleLine && (
                <TextStretcher>{`@${(renderValue ?? ((x) => x.toFixed(2)))(singleLine.points[singleLine.points.length - 1].y)}${lines[0].unit ?? ""}`}</TextStretcher>
              )}
            </div>

            {lines.length > 1 && (
              <LegendOrdinal
                scale={ordinalScale}
                labelFormat={(label) => label}
                shape="line"
                style={{
                  fontSize: "1em",
                }}
              />
            )}
          </>
        )}
      </div>
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

  return (
    <ParentSize key={forceRender} className="w-full h-full">
      {({ width, height }) => <LineGraphInner width={width} height={height} {...props} />}
    </ParentSize>
  );
};
