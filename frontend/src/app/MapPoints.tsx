import map from "./distro-map-light.png";
export interface GeolocatedNode {
	id: string;
	ip: string;
	latitude: number;
	longitude: number;
	country: string;
}

export const MapPoints: React.FC<{
	coordinates: Array<GeolocatedNode>;
}> = ({ coordinates }) => {
	const projectToCylindricalEquidistant = (
		lat: number,
		lng: number,
	): { x: number; y: number } => {
		// Cylindrical Equidistant projection formulas
		const x = (lng + 180) / 360; // x coordinate (0-1)
		const y = (90 - lat) / 180; // y coordinate (0-1)

		return {
			x: x * 100,
			y: y * 100,
		};
	};

	return (
		<div className="relative inline-block">
			<img src={map.src} alt="Nodes Map" className="w-full h-auto" />
			<svg className="absolute w-full h-full top-0 left-0">
				{coordinates
					.flatMap((from, index) =>
						coordinates.map((to, index2) => ({
							lat1: from.latitude,
							lon1: from.longitude,
							lat2: to.latitude,
							lon2: to.longitude,
							inds: [index, index2].toSorted().join(":"),
						})),
					)
					.filter(
						(item, index, self) =>
							index === self.findIndex((t) => t.inds === item.inds),
					)
					.map((line) => {
						const { x: x1, y: y1 } = projectToCylindricalEquidistant(
							line.lat1,
							line.lon1,
						);
						const { x: x2, y: y2 } = projectToCylindricalEquidistant(
							line.lat2,
							line.lon2,
						);
						return (
							<line
								key={line.inds}
								x1={`${x1}%`}
								y1={`${y1}%`}
								x2={`${x2}%`}
								y2={`${y2}%`}
								className="stroke-orange-500 stroke-[0.1]"
							/>
						);
					})}
			</svg>
			{coordinates.map((coord, index) => {
				const { x, y } = projectToCylindricalEquidistant(
					coord.latitude,
					coord.longitude,
				);

				return (
					<div
						key={index}
						className="absolute w-1 h-1 bg-orange-500 rounded-full transform -translate-x-1/2 -translate-y-1/2"
						style={{
							left: `${x}%`,
							top: `${y}%`,
						}}
					/>
				);
			})}
		</div>
	);
};
