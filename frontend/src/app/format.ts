export function formatBytes(bytes: number): string {
	if (Number.isNaN(bytes)) {
		return "0 B";
	}
	const KB = 1024.0;
	const MB = KB * 1024.0;
	const GB = MB * 1024.0;
	const TB = GB * 1024.0;
	const PB = TB * 1024.0;

	if (bytes < KB) {
		return `${bytes} B`;
	}
	if (bytes < MB) {
		return `${(bytes / KB).toFixed(2)} KB`;
	}
	if (bytes < GB) {
		return `${(bytes / MB).toFixed(2)} MB`;
	}
	if (bytes < TB) {
		return `${(bytes / GB).toFixed(2)} GB`;
	}
	if (bytes < PB) {
		return `${(bytes / TB).toFixed(2)} TB`;
	}
	return `${(bytes / PB).toFixed(2)} PB`;
}

export function formatNumber(num: number, decimals: number): string {
	const suffixes = ["", "k", "m", "b", "t", "q"];
	const suffixThresholds = [1, 1e3, 1e6, 1e9, 1e12, 1e15];

	if (num < 0) {
		return `-${formatNumber(-num, decimals)}`;
	}

	if (num < 1000) {
		return num.toString();
	}

	let suffixIndex = suffixes.length - 1;
	while (suffixIndex > 0 && num < suffixThresholds[suffixIndex]) {
		suffixIndex--;
	}

	const scaledNum = num / suffixThresholds[suffixIndex];
	const roundedNum = Math.floor(scaledNum * 10) / 10;

	return roundedNum.toFixed(decimals) + suffixes[suffixIndex];
}

export function formatTimeRemaining(seconds: number): string {
	if (seconds < 0) {
		return "00:00:00";
	}

	const days = Math.floor(seconds / (24 * 60 * 60));
	const hours = Math.floor((seconds % (24 * 60 * 60)) / (60 * 60));
	const minutes = Math.floor((seconds % (60 * 60)) / 60);

	const paddedDays = days.toString().padStart(2, "0");
	const paddedHours = hours.toString().padStart(2, "0");
	const paddedMinutes = minutes.toString().padStart(2, "0");

	return `${paddedDays}d:${paddedHours}h:${paddedMinutes}m`;
}
