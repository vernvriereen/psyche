"use client";
import {
	Children,
	ReactElement,
	type ReactNode,
	cloneElement,
	isValidElement,
	useCallback,
	useEffect,
	useLayoutEffect,
	useRef,
	useState,
} from "react";
import { type GraphLine, ResponsiveLineGraph } from "./Chart";
import { distroLogo } from "./distro-logo";
import { lookupIp } from "./geoip";
import { type WandBData, getData } from "./wandb";

import { Box } from "./Box";
import { LoadingScreen } from "./LoadingScreen";
import { TrainingProgress } from "./Progress";
import { Run } from "./Run";
import { formatBytes, formatNumber } from "./format";

export default function App() {
	const [wandbRun, setWandbRun] = useState<WandBData | null>(null);
	const [fading, setFading] = useState<"loading" | "fading" | false>("loading");
	const [error, setError] = useState(false);
	const fetchWandbData = useCallback(() => {
		getData("nous_research", "distro-live-test", "15b-100bt-28", 5000).then(
			(data) => {
				if (data) {
					setWandbRun(data);
				} else {
					setError(true);
				}
			},
		);
	}, []);

	useEffect(() => {
		fetchWandbData();
		if (!error) {
			const interval = setInterval(() => {
				fetchWandbData();
			}, 60_000);
			return () => clearInterval(interval);
		}
	}, [fetchWandbData, error]);

	useEffect(() => {
		if (
			Object.keys(wandbRun?.summary ?? {}).length > 0 &&
			fading === "loading"
		) {
			setFading("fading");
			setTimeout(() => {
				setFading(false);
			}, 2000);
		}
	}, [wandbRun, fading]);

	return (
		<>
			{fading && (
				<LoadingScreen fading={fading} warmupRun={wandbRun} error={error} />
			)}
			{wandbRun && Object.keys(wandbRun?.summary ?? {}).length > 0 && (
				<Run run={wandbRun} clipFirstEvalsN={10} tokensPerBatch={2048} />
			)}
		</>
	);
}
