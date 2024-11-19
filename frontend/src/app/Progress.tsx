import { useLayoutEffect, useRef, useState } from "react";
import { formatNumber, formatTimeRemaining } from "./format";

export function TrainingProgress({
	numCompletedTokens,
	numTotalTokens,
	tokensPerSecond,
}: {
	numCompletedTokens: number;
	numTotalTokens: number;
	tokensPerSecond: number;
}) {
	const estTimeRemaining =
		(numTotalTokens - numCompletedTokens) / tokensPerSecond;

	return (
		<div className="flex flex-col py-4">
			<div className="flex flex-row justify-between">
				<div>TRAINING PROGRESS</div>
				<div className="flex flex-row gap-6">
					<div>
						{formatNumber(numCompletedTokens, 1)}&nbsp;/&nbsp;
						{formatNumber(numTotalTokens, 0)}
					</div>
					<div>
						{((numCompletedTokens / numTotalTokens) * 100).toFixed(2)}% COMPLETE
					</div>
					<div>{`${formatTimeRemaining(estTimeRemaining)} LEFT`}</div>
				</div>
			</div>
			<BucketedProgressBar total={numTotalTokens} value={numCompletedTokens} />
		</div>
	);
}

function BucketedProgressBar({
	total,
	value,
}: { total: number; value: number }) {
	const [divisions, setDivisions] = useState(1);
	const containerRef = useRef<HTMLDivElement>(null);

	useLayoutEffect(() => {
		const updateDivisions = () => {
			if (containerRef.current) {
				const width = containerRef.current.offsetWidth;
				const maxDivisions = Math.floor(width / 30);
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

	return (
		<div
			ref={containerRef}
			className="flex flex-row justify-between w-full h-14"
		>
			{Array.from({ length: divisions }, (_, index) => {
				return (
					<div
						// biome-ignore lint/suspicious/noArrayIndexKey: this is correct, it's just a list.
						key={index}
						className={`border-2 border-primary h-full w-full mx-0.5 ${index === filledDivisions - 1 ? "animate-pulse" : ""} ${index < filledDivisions ? "bg-primary" : "bg-backdrop"}`}
					/>
				);
			})}
		</div>
	);
}
