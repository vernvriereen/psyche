interface PlusSymbolProps {
	x: number;
	y: number;
	size: number;
	className?: string;
	thickness: number;
}

const PlusSymbol: React.FC<PlusSymbolProps> = ({
	x,
	y,
	size = 5,
	thickness = 2,
	className = "stroke-grid",
}) => {
	const horizontalLine = {
		x1: x - size,
		x2: x + size,
		y1: y,
		y2: y,
	};

	const verticalLine = {
		x1: x,
		x2: x,
		y1: y - size,
		y2: y + size,
	};

	return (
		<g>
			<line {...verticalLine} className={className} strokeWidth={thickness} />
			<line {...horizontalLine} className={className} strokeWidth={thickness} />
		</g>
	);
};

const SplitPlusSymbol: React.FC<
	PlusSymbolProps & {
		half: "left" | "right" | "full";
		centerLineThickness: number;
	}
> = ({
	half,
	x,
	y,
	size,
	thickness,
	centerLineThickness,
	className = "stroke-grid",
}) => {
	const horizontalLineLeft = {
		x1: x - size - thickness - centerLineThickness / 2,
		x2: x - thickness - centerLineThickness / 2,
		y1: y,
		y2: y,
	};
	const horizontalLineRight = {
		x1: x + thickness + centerLineThickness / 2,
		x2: x + size + thickness + centerLineThickness / 2,
		y1: y,
		y2: y,
	};

	const verticalLineLeft = {
		x1: x - thickness - centerLineThickness / 2,
		x2: x - thickness - centerLineThickness / 2,
		y1: y - size,
		y2: y + size,
	};
	const verticalLineRight = {
		x1: x + thickness + centerLineThickness / 2,
		x2: x + thickness + centerLineThickness / 2,
		y1: y - size,
		y2: y + size,
	};

	return (
		<g>
			{half !== "right" && (
				<>
					<line
						{...verticalLineLeft}
						className={className}
						strokeWidth={thickness}
					/>
					<line
						{...horizontalLineLeft}
						className={className}
						strokeWidth={thickness}
					/>{" "}
				</>
			)}

			{half !== "left" && (
				<>
					<line
						{...verticalLineRight}
						className={className}
						strokeWidth={thickness}
					/>
					<line
						{...horizontalLineRight}
						className={className}
						strokeWidth={thickness}
					/>
				</>
			)}
		</g>
	);
};
export const GridOfPlusSymbols: React.FC<{
	verticalLinePositions: number[];
	horizontalLinePositions: number[];
	xScale: (value: number) => number;
	yScale: (value: number) => number;
	size: number;
	thickness: number;
	centerLineThickness: number;
}> = ({
	verticalLinePositions,
	horizontalLinePositions,
	xScale,
	yScale,
	size,
	thickness,
	centerLineThickness,
}) => {
	return (
		<>
			{verticalLinePositions.slice(0, -1).map((x, i) => {
				const midX = xScale((x + verticalLinePositions[i + 1]) / 2);
				return horizontalLinePositions.map((yPos, rowIndex) => (
					<PlusSymbol
						// biome-ignore lint/suspicious/noArrayIndexKey: these are actually only indexed by key, this is safe.
						key={`full-plus-${i}-${rowIndex}`}
						x={midX}
						y={yScale(yPos)}
						size={size}
						thickness={thickness}
					/>
				));
			})}

			{verticalLinePositions.map((x, i) => {
				return horizontalLinePositions.map((yPos, rowIndex) => (
					<SplitPlusSymbol
						// biome-ignore lint/suspicious/noArrayIndexKey: these are actually only indexed by key, this is safe.
						key={`half-plus-${i}-${rowIndex}`}
						x={xScale(x)}
						y={yScale(yPos)}
						size={size}
						thickness={thickness}
						centerLineThickness={centerLineThickness}
						half={
							i === 0
								? "right"
								: i === verticalLinePositions.length - 1
									? "left"
									: "full"
						}
					/>
				));
			})}
		</>
	);
};
