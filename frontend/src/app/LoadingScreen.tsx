import type { WandBData } from "./wandb";

export function LoadingScreen({
	fading,
	warmupRun,
	error,
}: {
	fading: "fading" | "loading";
	warmupRun: WandBData | null;
	error: boolean;
}) {
	return (
		<div
			className={`absolute w-screen h-screen flex flex-col items-center justify-center ${fading === "fading" ? "animate-fadeOut" : ""}`}
		>
			loading...
		</div>
	);
}
