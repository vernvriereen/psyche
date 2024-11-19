/** @type {import('tailwindcss').Config} */
module.exports = {
	content: ["./src/**/*.{ts,tsx}"],
	theme: {
		extend: {
			colors: {
				backdrop: "var(--color-backdrop)",
				primary: "var(--color-primary)",
			},
		},

		fontFamily: {
			monoubuntu: ["var(--font-ubuntu-mono)"],
		},
	},
	plugins: [],
};
