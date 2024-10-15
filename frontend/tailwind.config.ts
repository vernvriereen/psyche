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
        primary: "#0671A8",
        bad: "#E80054",
        good: "#81B56B",
        grid: "#6979C2",
      },
      // eva theme
      // colors: {
      //   backdrop: "black",
      //   primary: "#fa860a",
      //   bad: "#E80054",
      //   good: "#83C297",
      //   grid: "#83C297",
      // },
    },
  },
  plugins: [],
} satisfies Config;
