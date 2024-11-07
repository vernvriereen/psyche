import { Axis } from "@visx/axis";
import { curveLinear } from "@visx/curve";
import { Group } from "@visx/group";
import { LegendOrdinal } from "@visx/legend";
import { ParentSize } from "@visx/responsive";
import { scaleLinear, scaleOrdinal, scalePower } from "@visx/scale";
import { Line, LinePath } from "@visx/shape";
import type React from "react";
import { useLayoutEffect, useState } from "react";
import { GridOfPlusSymbols } from "./ChartPlus";
import { TextStretcher } from "./TextStretcher";
import { twStrokeToColor } from "./palette";
import useTailwind from "./tailwind";

interface DataPoint {
  x: number;
  y: number;
}

interface LineGraphProps {
  lines: Array<{ points: DataPoint[]; label: string; className: string; unit?: string }>;
  numXMarkers?: number;
  numYMarkers?: number;
  title?: string | [string, string];
  scale?: "linear" | "power";
}

const LineGraphInner: React.FC<LineGraphProps & { width: number; height: number }> = ({
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
    domain: lines.map((line) => `${line.label}: ${line.points.at(-1)?.y.toFixed(3)}${line.unit ?? ""}`),
    range: lines.map((line) => twStrokeToColor(line.className, tw)),
  });

  return (
    <div className="relative w-full h-full">
      <svg width={width} height={height} className="absolute">
        <Group left={margin.left + padding.left} top={padding.bottom}>
          {verticalLinePositions.map((x, i) => (
            <line
              key={`vertical-${i}`}
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
          to={{ x: padding.left * 1.5, y: height - margin.bottom - padding.bottom * 3 }}
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
                ? `${value.valueOf() >= 0 ? "" : "-"}${value.valueOf().toFixed(2).toString().slice(0, 4)}`
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
      </svg>
      <div className="absolute top-0 bg-backdrop/70" style={{ left: `${padding.left + margin.left + 12}px` }}>
        {title && (
          <>
            <div className=" w-32">
              <TextStretcher className="text-2xl p-2 h-12 border-2 rounded-md border-primary">
                {typeof title === "string" ? title : title[0]}
              </TextStretcher>
              {typeof title !== "string" && <TextStretcher className="my-2 h-12">{title[1]}</TextStretcher>}
              {lines.length === 1 && lines[0].points.length > 0 && (
                <TextStretcher>{`@${lines[0].points.at(-1)!.y.toFixed(2)}${lines[0].unit ?? ""}`}</TextStretcher>
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
