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
import { CoolTickMarks } from "./CoolTickMarks";
import globeTexture from "./assets/transparentglobe.png";
import { palette, twStrokeToColor } from "./palette";
import useTailwind from "./tailwind";
import type { PsycheStats, PyscheNode } from "./types/psyche";
import { DisplayP3ColorSpace, DoubleSide, LinearSRGBColorSpace, MeshBasicMaterial, NearestFilter, TextureLoader } from "three";

interface NodeConn {
  from: PyscheNode;
  to: PyscheNode;
}

function uuidToArrayItem<T>(uuid: string, array: T[]) {
  const color = BigInt(`0x${uuid.replaceAll("-", "")}`) % 0x1000000n;
  const item = array[Number(color % BigInt(array.length))];
  return item;
}

const globeMaterial = new MeshBasicMaterial({
  alphaTest: 0.9,
  // side: DoubleSide,
});

new TextureLoader().load(globeTexture, texture => {
  texture.minFilter = NearestFilter;
  texture.magFilter = NearestFilter;
  texture.colorSpace = ""
  globeMaterial.map = texture;
});

export function TrainersMap({ run }: { run: PsycheStats }) {
  const containerEl: RefObject<HTMLDivElement> = useRef(null);
  const globeEl: MutableRefObject<GlobeMethods | undefined> = useRef(undefined);
  useEffect(() => {
    // aim at continental US centroid
    globeEl.current?.pointOfView({ lat: 39.6, lng: -98.5, altitude: 2 });
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
    return run.nodes
      .flatMap((n) =>
        n.connections.map((to) => [n, run.nodes.find((n) => n.id === to)].toSorted() as [PyscheNode, PyscheNode]),
      )
      .filter((l, i, arr) => arr.findIndex((r) => r[0] === l[0] && r[1] === l[1]) === i)
      .map(([from, to]) => ({ from, to }));
  }, [run]);

  const tw = useTailwind();

  const primary = tw.theme.colors.primary;

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
          arcStartLat={((n: NodeConn) => n.from.location.lat) as any}
          arcStartLng={((n: NodeConn) => n.from.location.lon) as any}
          arcEndLat={((n: NodeConn) => n.to.location.lat) as any}
          arcEndLng={((n: NodeConn) => n.to.location.lon) as any}
          arcDashLength={2}
          arcDashGap={0}
          arcColor={() => primary}
          pointsData={run.nodes}
          pointLat={((p: PyscheNode) => p.location.lat) as any}
          pointLng={((p: PyscheNode) => p.location.lon) as any}
          pointColor={((p: PyscheNode) => twStrokeToColor(uuidToArrayItem(p.id, palette), tw)) as any}
        />
      </div>
    </div>
  );
}
