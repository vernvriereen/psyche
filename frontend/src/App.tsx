import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { ResponsiveLineGraph } from "./Chart";
import { TextStretcher } from "./TextStretcher";
import { TrainersMap } from "./TrainersMap";
import nousGirl from "./assets/nousgirl.png";
import { lerpColor } from "./color";
import { formatNumber } from "./formatNumber";
import { palette } from "./palette";
import type { PsycheStats, PyscheNode, StepStats } from "./types/psyche";
import { type WandBHistoryItem, getData } from "./wandb";

function randomLocation() {
  return {
    lat: Math.random() * 360 - 180,
    lon: (Math.acos(2 * Math.random() - 1) * 170.12) / Math.PI - 85.06,
  };
}

function randomNodes() {
  const nodes: PyscheNode[] = Array.from({ length: 16 }, (_, i) => ({
    location: randomLocation(),
    index: i,
    id: crypto.randomUUID(),
    connections: [],
  }));
  for (const n of nodes) {
    const num = Math.round(Math.random() * 5);
    n.connections = Array.from({ length: num }, () => nodes[Math.floor(Math.random() * nodes.length)].id);
  }
  return nodes;
}

export const App = () => {
  const [wandbData, setWandbData] = useState<Array<WandBHistoryItem>>([]);
  useEffect(() => {
    getData("nous_research", "psyche", "1b-100bt-32", 5000).then((data) => setWandbData(data));
  }, []);

  const nodes = useMemo<PyscheNode[]>(randomNodes, []);

  const run: PsycheStats = useMemo(
    () => ({
      nodes,
      coordinator: {
        runId: "DisTrO-llama3-405b-01",
        batchesPerRound: 128,
        roundHeight: 40,
        totalBatches: 100_000,
        tokensPerBatch: 2048,

        epoch: wandbData.at(-1)?.["coordinator/epoch"] ?? 0,
        startTime: new Date(),
        stats: wandbData.map(
          (data) =>
            ({
              step: data["coordinator/round"],
              evals: {
                hellaswag: data["eval/hellaswag"],
                "mmlu pro": data["eval/mmlu_pro"],
                "arc easy": data["eval/arc_easy"],
                "arc challenge": data["eval/arc_challenge"],
              },
              certainty: data["train/certainty"],
              loss: data["train/loss"],
              tokensPerSecond: data["train/tokens_per_sec"],
            }) satisfies StepStats,
        ),
      },
    }),
    [nodes, wandbData],
  );

  const numTotalTokens = run.coordinator.totalBatches * run.coordinator.tokensPerBatch;
  const numCompletedTokens =
    run.coordinator.batchesPerRound * run.coordinator.roundHeight * run.coordinator.tokensPerBatch;

  const evals = run.coordinator.stats.length
    ? (Object.keys(run.coordinator.stats[0].evals) as Array<keyof (typeof run.coordinator.stats)[number]["evals"]>).map(
      (evalName, index) => ({
        points: run.coordinator.stats
          .map((stat) => ({
            x: stat.step,
            y: stat.evals[evalName]!,
          }))
          .filter(({ y }) => y !== undefined),
        className: palette[index],
        label: evalName,
      }),
    )
    : [];

  return (
    <div className="font-eva h-screen w-screen p-4 text-primary flex flex-col">
      <div className="w-full flex justify-center">
        <img src={nousGirl} className="h-16" />
        <TextStretcher className="flex-grow text-xl h-16 pb-2">
          {`PSYCHE : DISTRIBUTED RUN _ ${run.coordinator.runId}`}
        </TextStretcher>
      </div>
      <p className="text-lg">
        Psyche is a distributed training framework. It interconnects globally distributed compute and trains
        state-of-the-art AI models at breakneck speeds. Psyche is powered by{" "}
        <a className="underline" href="https://github.com/NousResearch/DisTrO">
          DisTrO
        </a>
        , the bleeding edge distributed training algorithm by Nous Research.
      </p>

      <div className="flex flex-col w-full grow gap-4">
        <TrainingProgress numCompletedTokens={numCompletedTokens} numTotalTokens={numTotalTokens} />
        <div className="flex-1 flex flex-row max-h-[20vh]">
          <div className="w-[40%] ">
            <TrainersMap run={run} /></div>
          <div className="text-xl w-[60%]">
            <RunMembers members={run.nodes} />
          </div>
        </div>
        <div className="flex-1 grid grid-cols-2 overflow-auto">
          <div className="grid grid-cols-1 gap-4">
            <ResponsiveLineGraph
              scale="power"
              title={["TRAINING", "CERTAINTY"]}
              lines={[
                {
                  points: run.coordinator.stats.map((s) => ({
                    x: s.step,
                    y: s.certainty * 100,
                  })),
                  className: palette[1],
                  label: "Certainty",
                  unit: "%"
                },
              ]}
            />
            <ResponsiveLineGraph
              title={["TRAINING", "LOSS"]}
              lines={[
                {
                  points: run.coordinator.stats.map((s) => ({
                    x: s.step,
                    y: s.loss,
                  })),
                  className: palette[0],
                  label: "Loss",
                },
              ]}
            />
          </div>
          <div className="grid grid-cols-2 gap-4">
            {evals.map((e) => (
              <ResponsiveLineGraph numXMarkers={2} numYMarkers={3} title={["EVALUATION", e.label]} lines={[e]} />
            ))}
          </div>
        </div>
      </div>
    </div>
  );
};

function TrainingProgress({
  numCompletedTokens,
  numTotalTokens,
}: { numCompletedTokens: number; numTotalTokens: number }) {
  return (
    <div className="flex flex-col py-4">
      <div className="w-full border-2 border-primary" />
      <div className="flex flex-row">
        <div className="flex flex-col text-center pr-4 text-xl">
          <div>PROGRESS</div>
          <TextStretcher className="w-[60%] m-auto">{`${((numCompletedTokens / numTotalTokens) * 100).toFixed(1)}%`}</TextStretcher>
          <div>
            {formatNumber(numCompletedTokens)}&nbsp;/&nbsp;{formatNumber(numTotalTokens)}
          </div>
        </div>
        <BucketedProgressBar total={numTotalTokens} value={numCompletedTokens} colors={[]} />
      </div>
      <div className="w-full border-2 border-primary" />
    </div>
  );
}

function RunMembers({ members }: { members: PyscheNode[] }) {
  return (<div>
    <div className="w-full border-2 border-primary mb-2"></div>
    <div className="grid grid-cols-[repeat(auto-fill,minmax(150px,1fr))] gap-2 p-2">
      {members.map((m) => (
        <div key={m.id}>
          <div className="flex relative h-10">
            <NodeStatus name={m.id} status={+m.id.replaceAll(/\D/g, "") % 6 !== 0 ? "done" : "bad"} />
          </div>
        </div>
      ))}
    </div></div>
  );
}

function NodeStatus({ name, status }: { name: string; status: "done" | "bad" }) {
  return (
    <div className="w-full relative">
      <div className="absolute h-4 left-[50%]">
        <div className="relative top-[-100%] left-[-50%] w-1 h-4 text-primary bg-primary" />
      </div>
      <div className={`text-center rounded w-full h-8 ${status === "done" ? "bg-good text-good" : "bg-bad text-bad"}`}>
        <span className="text-black">{name.slice(0, 7)}</span>
      </div>
    </div>
  );
}

function BucketedProgressBar({ total, value }: { total: number; value: number, colors?: string[] }) {
  const [divisions, setDivisions] = useState(1);
  const containerRef = useRef<HTMLDivElement>(null);

  useLayoutEffect(() => {
    const updateDivisions = () => {
      if (containerRef.current) {
        const width = containerRef.current.offsetWidth;
        const maxDivisions = Math.floor(width / 12);
        setDivisions(Math.min(maxDivisions, total));
      }
    };

    updateDivisions();
    window.addEventListener("resize", updateDivisions);
    return () => window.removeEventListener("resize", updateDivisions);
  }, [total]);

  const filledDivisions = Math.round((value / total) * divisions);

  const start = {
    r: 73,
    g: 168,
    b: 137,
  };
  const end = {
    r: 121,
    g: 11,
    b: 176,
  };
  return (
    <div ref={containerRef} className="flex flex-row justify-between w-full h-full">
      {Array.from({ length: divisions }, (_, index) => {
        const { r, g, b } = lerpColor(start, end, index / divisions);
        return (
          <div
            key={index}
            style={
              index < filledDivisions
                ? {
                  backgroundColor: `rgb(${r}, ${g}, ${b})`,
                }
                : {}
            }
            className={`h-full min-w-2 ${index === filledDivisions - 1 ? "animate-pulse" : ""}`}
          />
        );
      })}
    </div>
  );
}
