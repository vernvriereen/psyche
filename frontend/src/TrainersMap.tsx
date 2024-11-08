import {
	type MutableRefObject,
	type RefObject,
	useCallback,
	useEffect,
	useLayoutEffect,
	useMemo,
	useRef,
	useState,
} from "react";
import Globe, { type GlobeMethods } from "react-globe.gl";
import {
	MeshBasicMaterial,
	NearestFilter,
	SRGBColorSpace,
	TextureLoader,
} from "three";
import { CoolTickMarks } from "./CoolTickMarks";
import globeTexture from "./assets/transparentglobe.png";
import useTailwind from "./tailwind";
import type { GeolocatedNode } from "./types";

const globeMaterial = new MeshBasicMaterial({
	alphaTest: 0.9,
});

new TextureLoader().load(globeTexture, (texture) => {
	texture.minFilter = NearestFilter;
	texture.magFilter = NearestFilter;
	texture.colorSpace = SRGBColorSpace;
	globeMaterial.map = texture;
});

export function TrainersMap({ nodes }: { nodes: Array<GeolocatedNode> }) {
	const containerEl: RefObject<HTMLDivElement> = useRef(null);
	const globeEl: MutableRefObject<GlobeMethods | undefined> = useRef(undefined);
	useEffect(() => {
		if (!globeEl.current) {
			return;
		}
		// aim at continental US centroid
		globeEl.current.pointOfView({ lat: 39.6, lng: -98.5, altitude: 2 });
		globeEl.current.controls().autoRotate = true;
		globeEl.current.controls().autoRotateSpeed = 0.5;
	}, []);

	const [size, setSize] = useState({ w: 0, h: 0 });

	const resizeCanvas = useCallback(() => {
		if (!containerEl.current) {
			return;
		}
		const w = containerEl.current.clientWidth;
		const h = containerEl.current.clientHeight;
		if (w !== size.w || h !== size.h) {
			setSize({ w, h });
		}
	}, [size]);

	useLayoutEffect(() => {
		resizeCanvas();
		window.addEventListener("resize", resizeCanvas);

		return () => {
			window.removeEventListener("resize", resizeCanvas);
		};
	}, [resizeCanvas]);

	const arcs = useMemo(() => {
		return nodes.flatMap((n) =>
			nodes
				.filter((no) => no !== n)
				.map((no) => ({
					from: n,
					to: no,
				})),
		);
	}, [nodes]);

	const tw = useTailwind();

	const primary = tw.theme.colors.primary;
	const good = tw.theme.colors.orange[400];

	return (
		<div className="w-full h-full relative" ref={containerEl}>
			<div className="absolute top-0 bottom-0 left-0 right-0">
				<CoolTickMarks className="text-grid" />
			</div>
			<div className="absolute top-0 bottom-0 left-0 right-0">
				<Globe
					width={size.w}
					height={size.h}
					ref={globeEl}
					globeMaterial={globeMaterial}
					showGraticules={false}
					showAtmosphere={false}
					atmosphereColor={primary}
					backgroundColor="rgba(0,0,0,0)"
					arcsData={arcs}
					arcStartLat={
						((n: { from: GeolocatedNode; to: GeolocatedNode }) =>
							n.from.latitude) as (n: object) => number
					}
					arcStartLng={
						((n: { from: GeolocatedNode; to: GeolocatedNode }) =>
							n.from.longitude) as (n: object) => number
					}
					arcEndLat={
						((n: { from: GeolocatedNode; to: GeolocatedNode }) =>
							n.to.latitude) as (n: object) => number
					}
					arcEndLng={
						((n: { from: GeolocatedNode; to: GeolocatedNode }) =>
							n.to.longitude) as (n: object) => number
					}
					arcDashLength={2}
					arcAltitude={0.05}
					arcDashGap={0}
					arcStroke={0.2}
					arcColor={() => good}
					pointAltitude={0.02}
					pointResolution={30}
					pointsData={nodes}
					pointLat={
						((p: GeolocatedNode) => p.latitude) as (n: object) => number
					}
					pointLng={
						((p: GeolocatedNode) => p.longitude) as (n: object) => number
					}
					// pointColor={((p: GeolocatedNode) => twStrokeToColor(idToArrayItem(p.id, palette), tw)) as any}
					pointColor={() => good}
					pointLabel={
						((p: GeolocatedNode) =>
							`<div class="bg-backdrop text-primary">node ${p.id.slice(0, 10)}</div>`) as (
							n: object,
						) => string
					}
				/>
			</div>
		</div>
	);
}
