import type useTailwind from "./tailwind";

export const palette = [
	"stroke-yellow-500",
	"stroke-sky-500",
	"stroke-violet-500",
	"stroke-lime-500",
	"stroke-red-500",
];

export function twStrokeToColor(
	className: string,
	tw: ReturnType<typeof useTailwind>,
): string {
	const stroke = className.split(" ").find((c) => c.startsWith("stroke-"));
	if (!stroke) {
		return "";
	}
	const [color, size] = stroke.replace("stroke-", "").split("-");

	const twColor = (
		tw.theme.colors as unknown as Record<
			string,
			{ [index: string]: string } | string
		>
	)[color];
	const realColor =
		typeof twColor === "object" && size ? twColor[size] : twColor;
	if (typeof realColor === "object") {
		throw new Error("invalid color string");
	}
	return realColor;
}
