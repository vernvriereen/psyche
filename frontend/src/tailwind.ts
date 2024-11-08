import { useMemo } from "react";
import resolveConfig from "tailwindcss/resolveConfig";
import tailwindConfig from "../tailwind.config";

export default function useTailwind() {
	return useMemo(() => resolveConfig(tailwindConfig), []);
}
