import { AxisBottom, AxisTop } from "@visx/axis";
import { Group } from "@visx/group";
import { ParentSize } from "@visx/responsive";
import { scaleLinear } from "@visx/scale";
import { Line } from "@visx/shape";
import type React from "react";

interface BottomAxisProps {
  width: number;
  height: number;
}

export const CoolTickMarksInner: React.FC<BottomAxisProps> = ({ width, height }) => {
  const xScale = scaleLinear<number>({
    domain: [0, 1],
    range: [0, width],
  });
  const xDomain = xScale.domain() as [number, number];
  const numXMarkers = 5;
  const xMarkersEvery = (xDomain[1] - xDomain[0]) / numXMarkers;

  // Meta ticks
  const megaTickValuesXAxis: number[] = [];
  for (let i = xDomain[0]; i <= xDomain[1]; i += xMarkersEvery) {
    megaTickValuesXAxis.push(i);
  }

  // Major ticks
  const majorTicksPerVerticalLine = 4;
  const majorTickValuesXAxis: number[] = [];
  for (let i = xDomain[0]; i <= xDomain[1]; i += xMarkersEvery / majorTicksPerVerticalLine) {
    majorTickValuesXAxis.push(i);
  }

  // Minor ticks
  const minorTicksPerMajorTick = 5;
  const allTickValuesXAxis: number[] = [];
  for (let i = xDomain[0]; i <= xDomain[1]; i += xMarkersEvery / (majorTicksPerVerticalLine * minorTicksPerMajorTick)) {
    allTickValuesXAxis.push(i);
  }

  const barWidth = 4;
  const tickLen = 2;
  const tickPad = barWidth + 3;

  return (
    <svg width={width} height={height} role="img" aria-label="plus-shaped grid">
      <Line from={{ x: 0, y: barWidth / 2 }} to={{ x: width, y: barWidth / 2 }} strokeWidth={4} stroke="currentColor" />
      <Line
        from={{ x: 0, y: height - barWidth / 2 }}
        to={{ x: width, y: height - barWidth / 2 }}
        strokeWidth={4}
        stroke="currentColor"
      />
      <Group top={tickPad}>
        <AxisBottom
          scale={xScale}
          tickValues={megaTickValuesXAxis}
          tickLength={tickLen * 12}
          strokeWidth={3}
          tickStroke="currentColor"
          hideAxisLine
          tickComponent={() => null}
        />
        <AxisBottom
          scale={xScale}
          tickValues={majorTickValuesXAxis}
          tickLength={tickLen * 4}
          strokeWidth={2}
          tickStroke="currentColor"
          hideAxisLine
          tickComponent={() => null}
        />
        <AxisBottom
          scale={xScale}
          tickValues={allTickValuesXAxis}
          tickLength={tickLen}
          strokeWidth={2}
          tickStroke="currentColor"
          hideAxisLine
          tickComponent={() => null}
        />
      </Group>
      <Group top={height - tickPad}>
        <AxisTop
          scale={xScale}
          tickValues={megaTickValuesXAxis}
          tickLength={tickLen * 12}
          strokeWidth={2}
          tickStroke="currentColor"
          hideAxisLine
          tickComponent={() => null}
        />
        <AxisTop
          scale={xScale}
          tickValues={majorTickValuesXAxis}
          tickLength={tickLen * 4}
          strokeWidth={2}
          tickStroke="currentColor"
          hideAxisLine
          tickComponent={() => null}
        />
        <AxisTop
          scale={xScale}
          tickValues={allTickValuesXAxis}
          tickLength={tickLen}
          strokeWidth={2}
          tickStroke="currentColor"
          hideAxisLine
          tickComponent={() => null}
        />
      </Group>
    </svg>
  );
};

export const CoolTickMarks: React.FC<{ className?: string }> = (props) => {
  return (
    <ParentSize className={`w-full h-full overflow-hidden ${props.className}`}>
      {({ width, height }) => <CoolTickMarksInner width={width} height={height} {...props} />}
    </ParentSize>
  );
};
