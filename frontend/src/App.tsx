import {
	useCallback,
	useEffect,
	useLayoutEffect,
	useMemo,
	useRef,
	useState,
} from "react";
import { type GraphLine, ResponsiveLineGraph } from "./Chart";
import { TextStretcher } from "./TextStretcher";
import { TrainersMap } from "./TrainersMap";
import nousGirl from "./assets/nousgirl.png";
import psycheLogo from "./assets/psyche.png";
import { lerpColor } from "./color";
import { formatNumber } from "./formatNumber";
import { lookupIp } from "./geoip";
import { palette } from "./palette";
import type { GeolocatedNode } from "./types";
import { type WandBData, getData } from "./wandb";

function RepeatElements({
	children,
	total,
}: { children: JSX.Element[]; total: number }) {
	const repeatedChildren: JSX.Element[] = [];
	for (let i = 0; i < total; i++) {
		const index = i % children.length;
		repeatedChildren.push(children[index]);
	}
	return repeatedChildren;
}

export const App = () => {
	const [wandbRun, setWandbRun] = useState<WandBData | null>(null);
	const [fading, setFading] = useState<"loading" | "fading" | false>("loading");
	const fetchWandbData = useCallback(() => {
		getData("nous_research", "distro-live-test", "15b-100bt-2", 5000).then(
			(data) => {
				setWandbRun(data);
			},
		);
	}, []);
	useEffect(() => {
		setInterval(() => {
			fetchWandbData();
		}, 60_000);
	}, [fetchWandbData]);

	useEffect(() => {
		if (wandbRun?.summary && fading === "loading") {
			setFading("fading");
			setTimeout(() => {
				setFading(false);
			}, 2000);
		}
	}, [wandbRun, fading]);

	return (
		<>
			{fading && <LoadingScreen fading={fading} warmupRun={wandbRun} />}
			{wandbRun?.summary && <Run run={wandbRun} clipFirstEvalsN={10} />}
		</>
	);
};

const Run: React.FC<{ run: WandBData; clipFirstEvalsN?: number }> = ({
	run,
	clipFirstEvalsN,
}) => {
	const [nodes, setNodes] = useState<Array<GeolocatedNode>>([]);
	useEffect(() => {
		let cancelled = false;
		(async () => {
			const promises = Object.entries(run.summary.p2p.nodes).map(
				async ([k, v]) => {
					const ips = v.ips.split(",");
					const goodIps = ips.filter(
						(ip) =>
							!ip.startsWith("10.") &&
							!(
								ip.startsWith("172.") &&
								+ip.split(".")[1] >= 16 &&
								+ip.split(".")[1] <= 31
							) &&
							!ip.startsWith("192.168"),
					);
					const ipResult = await lookupIp(goodIps[0].split(":")[0]);
					return { id: k, ip: goodIps[0], ...ipResult };
				},
			);
			const nodes = (await Promise.all(promises))
				.filter((n): n is GeolocatedNode => n.latitude !== undefined)
				.map((n) => {
					// fudge locations to prevent bunching
					n.latitude += (Math.random() - 0.5) * 2.5;
					n.longitude += (Math.random() - 0.5) * 2.5;
					return n;
				});
			if (!cancelled) {
				setNodes(nodes);
			}
		})();
		return () => {
			cancelled = true;
		};
	}, [run]);

	const numTotalTokens =
		run.config.total_steps *
		run.config.data_indicies_per_batch *
		run.config.batches_per_round;
	const numCompletedTokens =
		run.summary.coordinator.round *
		run.config.data_indicies_per_batch *
		run.config.batches_per_round;

	const evals = (
		Object.keys(run.summary.eval) as Array<keyof typeof run.summary.eval>
	).map(
		(evalName, index) =>
			({
				points: run.history
					.slice(clipFirstEvalsN ?? 0, -1)
					.map((historyItem) => ({
						x: historyItem.coordinator.round,
						y: historyItem.eval[evalName],
					}))
					.filter(({ y }) => y !== undefined)
					// evals are %
					.map(({ x, y }) => ({ x, y: y * 100 })),
				className: palette[index],
				label: evalName.toUpperCase().replaceAll("_", " "),
				unit: "%",
			}) satisfies GraphLine,
	);

	return (
		<div className="font-black h-screen w-screen p-4 text-primary flex flex-col animate-fadeIn">
			<div className="w-full flex justify-center font-eva pb-2">
				<img alt="Nous Girl Logo" src={nousGirl} className="h-16" />
				<img alt="Nous Psyche Logo" src={psycheLogo} className="h-16" />
				<TextStretcher className="font-normal text-plain flex-grow text-xl h-16 pb-2 px-4">
					{`NOUS PSYCHE : DISTRIBUTED TRAINING RUN _ ${run.displayName}`}
				</TextStretcher>
				<img alt="Nous Psyche Logo" src={psycheLogo} className="h-16" />
				<img alt="Nous Girl Logo" src={nousGirl} className="h-16" />
			</div>
			<div className="text-plain text-lg font-thin font-eva">
				<p>
					Psyche is a DisTrO-native distributed training framework for AI
					models. It interconnects globally dispersed compute and trains models
					at breakneck speed, with high quality and accuracy.
				</p>
				<p>
					Psyche is powered by{" "}
					<a
						className="underline"
						href="https://github.com/NousResearch/DisTrO"
					>
						Nous DisTrO
					</a>
					, the distributed training optimizer algorithm built by Nous Research.
				</p>
			</div>

			<div className="flex flex-col w-full grow gap-4">
				<TrainingProgress
					numCompletedTokens={numCompletedTokens}
					numTotalTokens={numTotalTokens}
				/>
				<div className="flex-1 flex flex-row max-h-[20vh]">
					<div className="w-[40%] ">
						<TrainersMap nodes={nodes} />
					</div>
					<div className="text-xl w-[60%]">
						<RunMembers nodes={run.summary.p2p.nodes} />
					</div>
				</div>
				<div className="flex-1 grid grid-cols-2 overflow-auto">
					<div className="grid grid-cols-1 gap-4">
						<ResponsiveLineGraph
							title={["TRAINING", "CONFIDENCE"]}
							lines={[
								{
									points: run.history.map((s) => ({
										x: s.coordinator.round,
										y: s.train.confidence * 100,
									})),
									className: palette[1],
									label: "Confidence",
									unit: "%",
								},
							]}
						/>
						<ResponsiveLineGraph
							title={["TRAINING", "LOSS"]}
							lines={[
								{
									points: run.history.map((s) => ({
										x: s.coordinator.round,
										y: s.train.loss,
									})),
									className: palette[0],
									label: "Loss",
								},
							]}
						/>
					</div>
					<div className="grid grid-cols-2 gap-4">
						{evals.map((e) => (
							<ResponsiveLineGraph
								key={e.label}
								numXMarkers={2}
								numYMarkers={3}
								title={["EVALUATION", e.label]}
								lines={[e]}
							/>
						))}
					</div>
				</div>
			</div>
		</div>
	);
};

function LoadingScreen({
	fading,
	warmupRun,
}: { fading: "fading" | "loading"; warmupRun: WandBData | null }) {
	return (
		<div
			className={`absolute w-screen h-screen flex flex-col items-center justify-center ${fading === "fading" ? "animate-fadeOut" : ""}`}
		>
			<div>
				<RepeatElements total={7}>
					<img
						src={nousGirl}
						className="h-[calc(40vw/7)] inline"
						alt="nous girl logo"
					/>
					<img
						src={psycheLogo}
						className="h-[calc(40vw/7)] inline"
						alt="nous psyche logo"
					/>
				</RepeatElements>
			</div>
			<div className="text-9xl font-eva text w-[40vw] p-[1vw]">
				<TextStretcher className="w-[10vw] h-[5vw] mt-[1vw]">
					NOUS
				</TextStretcher>
				<TextStretcher className="w-[20vw] h-[5vw] mt-[1vw]">
					PSYCHE
				</TextStretcher>
				<TextStretcher className="w-full h-[15vw]">
					INITIALIZING...
				</TextStretcher>
				{warmupRun && (
					<TextStretcher className="w-full h-[3vw] mt-[2vw]">
						{`RUN ${warmupRun.displayName} WARMING UP`}
					</TextStretcher>
				)}
			</div>
			<div className="pt-4">
				<RepeatElements total={7}>
					<img
						src={psycheLogo}
						className="h-[calc(40vw/7)] inline"
						alt="nous psyche logo"
					/>
					<img
						src={nousGirl}
						className="h-[calc(40vw/7)] inline"
						alt="nous girl logo"
					/>
				</RepeatElements>
			</div>
		</div>
	);
}

function TrainingProgress({
	numCompletedTokens,
	numTotalTokens,
}: { numCompletedTokens: number; numTotalTokens: number }) {
	return (
		<div className="flex flex-col py-4 font-eva">
			<div className="w-full border-2 border-primary" />
			<div className="flex flex-row">
				<div className="flex flex-col text-center pr-4 text-xl">
					<div>PROGRESS</div>
					<TextStretcher className="w-[60%] m-auto">{`${((numCompletedTokens / numTotalTokens) * 100).toFixed(2)}%`}</TextStretcher>
					<div>
						{formatNumber(numCompletedTokens)}&nbsp;/&nbsp;
						{formatNumber(numTotalTokens)}
					</div>
				</div>
				<BucketedProgressBar
					total={numTotalTokens}
					value={numCompletedTokens}
					colors={[]}
				/>
			</div>
			<div className="w-full border-2 border-primary" />
		</div>
	);
}

function RunMembers({
	nodes,
}: { nodes: Record<string, { bandwidth: number }> }) {
	const numNodes = Object.keys(nodes).length;
	// we multiply all the #s * the number of nodes, since they're all doing this much transfer to every other node, and we're only loggin data from one.
	return (
		<div className="px-8">
			<div className="w-full border-2 border-primary mb-2 text-center bg-primary text-backdrop p-2 font-eva">
				<TextStretcher className="pb-2">
					ESTIMATED NETWORK TRANSFER RATE
				</TextStretcher>
				{convertBytes(
					Object.values(nodes).reduce((a, b) => a + b.bandwidth, 0) * numNodes,
				)}
				/s
			</div>
			<div className="grid grid-cols-[repeat(auto-fill,minmax(150px,1fr))] gap-2 p-2">
				{Object.entries(nodes).map(([id, { bandwidth }]) => (
					<div key={id} className="relative h-18">
						<div>
							<NodeStatus name={id} bandwidth={bandwidth} />
						</div>
					</div>
				))}
			</div>
		</div>
	);
}

function convertBytes(bytes: number): string {
	if (Number.isNaN(bytes)) {
		return "0 B";
	}
	const KB = 1024.0;
	const MB = KB * 1024.0;
	const GB = MB * 1024.0;
	const TB = GB * 1024.0;
	const PB = TB * 1024.0;

	if (bytes < KB) {
		return `${bytes} B`;
	}
	if (bytes < MB) {
		return `${(bytes / KB).toFixed(2)} KB`;
	}
	if (bytes < GB) {
		return `${(bytes / MB).toFixed(2)} MB`;
	}
	if (bytes < TB) {
		return `${(bytes / GB).toFixed(2)} GB`;
	}
	if (bytes < PB) {
		return `${(bytes / TB).toFixed(2)} TB`;
	}
	return `${(bytes / PB).toFixed(2)} PB`;
}

function NodeStatus({ name, bandwidth }: { name: string; bandwidth: number }) {
	return (
		<div className="w-full relative">
			<div className="absolute h-4 left-[50%]">
				<div className="relative top-[-100%] left-[-50%] w-1 h-4 text-primary bg-primary" />
			</div>
			<div className="flex flex-col items-center justify-center rounded w-full h-16 bg-primary text-backdrop">
				<div>{name.slice(0, 7)}</div>
				<div className="text-sm">{convertBytes(bandwidth)}/s</div>
			</div>
		</div>
	);
}

function BucketedProgressBar({
	total,
	value,
}: { total: number; value: number; colors?: string[] }) {
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

	const filledDivisions = Math.min(
		Math.round((value / total) * divisions) + 1,
		divisions,
	);

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
		<div
			ref={containerRef}
			className="flex flex-row justify-between w-full h-full"
		>
			{Array.from({ length: divisions }, (_, index) => {
				const { r, g, b } = lerpColor(start, end, index / divisions);
				return (
					<div
						key={`${r}${g}${b}`}
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
