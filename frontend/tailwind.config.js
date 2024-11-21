/** @type {import('tailwindcss').Config} */
module.exports = {
  content: ["./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        backdrop: "var(--color-backdrop)",
        primary: "var(--color-primary)",
        plain: "var(--color-plain)",
      },
    },

    fontFamily: {
      monoubuntu: ["var(--font-ubuntu-mono)"],
    },
  },
  plugins: [],
};
