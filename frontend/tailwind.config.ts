import type { Config } from "tailwindcss";

export default {
	content: ["./src/**/*.{html,ts,tsx}"],
	theme: {
		extend: {
			fontFamily: {
				eva: ["Eva", "sans-serif"],
			},
			// nous theme
			colors: {
				backdrop: "white",
				plain: "black",
				primary: "#0671A8",
				bad: "#E80054",
				good: "#81B56B",
				grid: "#6979C2",
			},
			// eva theme
			// colors: {
			//   backdrop: "black",
			//   plain: "white",
			//   primary: "#fa860a",
			//   bad: "#E80054",
			//   good: "#83C297",
			//   grid: "#83C297",
			// },

			animation: {
				fadeIn: "fadeIn 2s ease-in-out",
				fadeOut: "fadeOut 2s ease-in-out",
			},

			keyframes: {
				fadeIn: {
					"0%": { opacity: "0" },
					"30%": { opacity: "0" },
					"100%": { opacity: "1" },
				},
				fadeOut: {
					from: { opacity: "1" },
					to: { opacity: "0" },
				},
			},
		},
	},
	plugins: [],
} satisfies Config;
