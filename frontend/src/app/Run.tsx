import { useEffect, useState } from "react";
import { Box } from "./Box";
import { type GraphLine, ResponsiveLineGraph } from "./Chart";
import { type GeolocatedNode, MapPoints } from "./MapPoints";
import { TrainingProgress } from "./Progress";
import { distroLogo } from "./distro-logo";
import { formatBytes, formatNumber } from "./format";
import { lookupIp } from "./geoip";
import type { WandBData } from "./wandb";

export const Run: React.FC<{
	run: WandBData;
	clipFirstEvalsN?: number;
	tokensPerBatch: number;
}> = ({ run, clipFirstEvalsN, tokensPerBatch }) => {
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
		run.config.batches_per_round *
		tokensPerBatch;

	const numCompletedTokens =
		run.summary._step *
		run.config.data_indicies_per_batch *
		run.config.batches_per_round *
		tokensPerBatch;

	const evals = (
		Object.keys(run.summary.eval) as Array<keyof typeof run.summary.eval>
	).map(
		(evalName) =>
			({
				points: run.history
					.slice(clipFirstEvalsN ?? 0, -1)
					.map((historyItem) => ({
						x: historyItem._step,
						y: historyItem.eval[evalName],
					}))
					.filter(({ y }) => y !== undefined)
					// evals are %
					.map(({ x, y }) => ({ x, y: y * 100 })),
				label: evalName.toUpperCase().replaceAll("_", " "),
				unit: "%",
				className: "stroke-primary",
			}) satisfies GraphLine,
	);

	return (
		<div className="p-4 pt-0 pb-2 text-primary font-monoubuntu flex flex-col h-full min-h-[100vh]">
			<Box
				title={
					<span className="flex flex-row justify-between">
						<span className="flex flex-row gap-6">
							<span>15b 100bt run</span>
							<span>colors</span>
						</span>
						<span>Nous DisTrO</span>
						<span>
							<span>GH</span>
							<span>TW</span>
							<span>BS</span>
						</span>
					</span>
				}
			>
				<div className="flex items-stretch justify-between min-h-[30vh] xl:flex-row flex-col">
					<div className="flex flex-col">
						<pre className="xl:text-[0.7em] lg:text-[0.9em] md:text-[0.7em] sm:text-[0.5em] text-[0.35em] font-mono xl:text-left text-center">
							{distroLogo}
						</pre>
						<div className="pt-2">
							<TrainingProgress
								numCompletedTokens={numCompletedTokens}
								numTotalTokens={numTotalTokens}
								tokensPerSecond={run.summary.train.tokens_per_sec}
							/>
						</div>
						<div className="leading-tight">
							ABOUT NOUS DISTRO: Training large scale neural networks typically
							involves sharing gradients between all accelerators, which
							necessitates specialized, high-speed interconnects. To address
							this, we introduce DisTrO, a family of architecture-agnostic and
							network-agnostic distributed optimizers that reduces the inter-GPU
							communication requirements by four to five orders of magnitude
							without relying on amortized analysis, enabling low-latency
							training of large neural networks on slow internet bandwidths with
							heterogeneous networking hardware. In this preliminary report we
							are excited to show the first and earliest empirical proof that
							DisTrO can match competitive LLM training in convergence rate
							while massively reducing the required bandwidth during
							pre-training of a 15b LLM. When using Distributed Data
							Parallelism, DisTrO may enable future large scale foundation model
							training to bypass the need for high-speed interconnects entirely.
						</div>
					</div>
					<div className="p-3 pl-6 min-w-[40vw] flex items-center">
						<Box title="Live Global Status" fullH={false}>
							<div className="h-fit py-8">
								<MapPoints coordinates={nodes} />
							</div>
						</Box>
					</div>
				</div>
			</Box>
			<div className="grow flex">
				<Box title="MODEL REASONING EVALUATIONS">
					<div className="flex flex-col lg:flex-row h-full lg:min-h-0 min-h-[50vh]">
						{evals.map((e) => (
							<ResponsiveLineGraph
								xLabel="step"
								key={e.label}
								title={e.label}
								lines={[e]}
							/>
						))}
					</div>
				</Box>
			</div>
			<div className="grow flex gap-4 flex-col lg:flex-row">
				<Box title="NETWORK SPEED">
					<div className="flex flex-col lg:flex-row h-full lg:min-h-0 min-h-[25vh]">
						<ResponsiveLineGraph
							xLabel="step"
							renderValue={(v) => formatNumber(v, 0)}
							title="TRAINING RATE"
							lines={[
								{
									className: "stroke-primary",
									label: "Tokens Per Second",
									unit: " tok/s",
									points: run.history.map((s) => ({
										x: s._step,
										y: s.train.tokens_per_sec,
									})),
								},
							]}
						/>
						<ResponsiveLineGraph
							xLabel="step"
							renderValue={formatBytes}
							title="BANDWIDTH"
							lines={[
								{
									className: "stroke-primary",
									label: "Node Bandwidth",
									unit: "/s",
									points: run.history
										.filter((s) => !!s.p2p?.nodes)
										.map((s) => ({
											x: s._step,
											y: Object.values(s.p2p.nodes)
												.map((v) => v.bandwidth)
												.filter((x) => !!x)
												.reduce((a, b) => a + b, 0),
										})),
								},
							]}
						/>
					</div>
				</Box>
				<Box title="Model Training">
					<div className="flex flex-col lg:flex-row h-full lg:min-h-0 min-h-[25vh]">
						<ResponsiveLineGraph
							xLabel="step"
							title="CONFIDENCE"
							lines={[
								{
									points: run.history.map((s) => ({
										x: s._step,
										y: s.train.confidence * 100,
									})),
									className: "stroke-primary",
									label: "Confidence",
									unit: "%",
								},
							]}
						/>
						<ResponsiveLineGraph
							xLabel="step"
							title="LOSS"
							lines={[
								{
									points: run.history.map((s) => ({
										x: s._step,
										y: s.train.loss,
									})),
									className: "stroke-primary",
									label: "Loss",
								},
							]}
						/>
					</div>
				</Box>
			</div>
		</div>
	);
};
