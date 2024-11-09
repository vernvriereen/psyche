import { defineConfig } from "@rsbuild/core";
import { pluginReact } from "@rsbuild/plugin-react";
import { pluginSvgr } from "@rsbuild/plugin-svgr";

export default defineConfig({
	plugins: [pluginReact(), pluginSvgr()],
	output: { assetPrefix: "./" },
	html: {
		title: "NOUS DisTro",
		// title: "NOUS PSYCHE",
		// favicon: "./src/assets/psyche.png",
		favicon: "./src/assets/nousgirl.png",
	},
});
