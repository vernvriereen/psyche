export function setTheme(dark?: boolean) {
	if (typeof window !== "undefined") {
		if (dark !== undefined) {
			localStorage.setItem("theme", dark ? "dark" : "light");
		}
		document.documentElement.classList.toggle(
			"dark",
			localStorage.getItem("theme") === "dark",
		);
	}
}
