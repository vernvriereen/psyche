type IpLookupResult = {
	latitude: number;
	longitude: number;
	country: string;
} | null;

const numToCountryCode = (num: number): string => {
	return String.fromCharCode(((num / 26) | 0) + 65, (num % 26) + 65);
};

const aton4 = (a: string): number => {
	const parts = a.split(/\./);
	if (parts.length !== 4) throw new Error("Invalid IPv4 address");
	return (
		((Number.parseInt(parts[0]) << 24) |
			(Number.parseInt(parts[1]) << 16) |
			(Number.parseInt(parts[2]) << 8) |
			Number.parseInt(parts[3])) >>>
		0
	);
};

const aton6Start = (a: string): bigint | number => {
	if (a.includes(".")) {
		return aton4(a.split(":").pop() || "");
	}
	const parts = a.split(/:/);
	const l = parts.length - 1;
	let r = 0n;

	if (l < 7) {
		const omitStart = parts.indexOf("");
		if (omitStart < 4) {
			const omitted = 8 - parts.length;
			const omitEnd = omitStart + omitted;
			for (let i = 7; i >= omitStart; i--) {
				parts[i] = i > omitEnd ? parts[i - omitted] : "0";
			}
		}
	}

	for (let i = 0; i < 4; i++) {
		if (parts[i]) {
			r += BigInt(Number.parseInt(parts[i], 16)) << BigInt(16 * (3 - i));
		}
	}
	return r;
};

const getUnderberFill = (num: string, len: number): string => {
	if (num.length > len) return num;
	return "_".repeat(len - num.length) + num;
};

const numberToDir = (num: number): string => {
	return getUnderberFill(num.toString(36), 2);
};

const TOP_URL = "https://cdn.jsdelivr.net/npm/@iplookup/geocode/";
const MAIN_RECORD_SIZE = 8;
const sleep = (ms: number): Promise<void> =>
	new Promise((resolve) => setTimeout(resolve, ms));

const downloadArrayBuffer = async (
	url: string,
	retry = 3,
): Promise<ArrayBuffer | null> => {
	try {
		const res = await fetch(url, { cache: "no-cache" });
		if (!res.ok) {
			if (res.status === 404) return null;
			if (retry) {
				await sleep(100 * (4 - retry) * (4 - retry));
				return downloadArrayBuffer(url, retry - 1);
			}
			return null;
		}
		return res.arrayBuffer();
	} catch (error) {
		if (retry) {
			await sleep(100 * (4 - retry) * (4 - retry));
			return downloadArrayBuffer(url, retry - 1);
		}
		return null;
	}
};

const downloadIdx = downloadArrayBuffer;

interface IndexMap {
	[key: number]: Uint32Array | BigUint64Array | undefined;
}

interface UrlMap {
	[key: number]: string;
}

const Idx: IndexMap = {};
const Url: UrlMap = { 4: TOP_URL, 6: TOP_URL };

const Preload: {
	[key: number]: Promise<Uint32Array | BigUint64Array | undefined>;
} = {
	4: downloadIdx(`${TOP_URL}4.idx`).then((buf) => {
		if (!buf) return undefined;
		Idx[4] = new Uint32Array(buf);
		return Idx[4];
	}),
	6: downloadIdx(`${TOP_URL}6.idx`).then((buf) => {
		if (!buf) return undefined;
		Idx[6] = new BigUint64Array(buf);
		return Idx[6];
	}),
};

export async function lookupIp(ipString: string): Promise<IpLookupResult> {
	let ip: number | bigint;
	let version: 4 | 6;
	let isv4 = true;

	if (ipString.includes(":")) {
		ip = aton6Start(ipString);
		version = ip.constructor === BigInt ? 6 : 4;
		if (version === 6) isv4 = false;
	} else {
		ip = aton4(ipString);
		version = 4;
	}

	const ipIndexes = Idx[version] || (await Preload[version]);
	if (!ipIndexes) return null;

	if (
		(typeof ip === "bigint" && typeof ipIndexes[0] === "number") ||
		(typeof ip === "number" && typeof ipIndexes[0] === "bigint")
	) {
		return null;
	}

	if (!(ip >= ipIndexes[0])) return null;

	let fline = 0;
	let cline = ipIndexes.length - 1;
	let line: number;

	for (;;) {
		line = (fline + cline) >> 1;
		if (ip < ipIndexes[line]) {
			if (cline - fline < 2) return null;
			cline = line - 1;
		} else {
			if (fline === line) {
				if (cline > line && ip >= ipIndexes[cline]) {
					line = cline;
				}
				break;
			}
			fline = line;
		}
	}

	const fileName = numberToDir(line);
	const dataBuffer = await downloadArrayBuffer(
		`${Url[version] + version}/${fileName}`,
	);
	if (!dataBuffer) return null;

	const ipSize = (version - 2) * 2;
	const recordSize = MAIN_RECORD_SIZE + ipSize * 2;
	const recordCount = dataBuffer.byteLength / recordSize;

	const startList = isv4
		? new Uint32Array(dataBuffer.slice(0, 4 * recordCount))
		: new BigUint64Array(dataBuffer.slice(0, 8 * recordCount));

	fline = 0;
	cline = recordCount - 1;

	for (;;) {
		line = (fline + cline) >> 1;
		if (ip < startList[line]) {
			if (cline - fline < 2) return null;
			cline = line - 1;
		} else {
			if (fline === line) {
				if (cline > line && ip >= startList[cline]) {
					line = cline;
				}
				break;
			}
			fline = line;
		}
	}

	const endIp = isv4
		? new Uint32Array(
				dataBuffer.slice(
					(recordCount + line) * ipSize,
					(recordCount + line + 1) * ipSize,
				),
			)[0]
		: new BigUint64Array(
				dataBuffer.slice(
					(recordCount + line) * ipSize,
					(recordCount + line + 1) * ipSize,
				),
			)[0];

	if (ip >= startList[line] && ip <= endIp) {
		const arr = new Int32Array(
			dataBuffer.slice(
				recordCount * ipSize * 2 + line * MAIN_RECORD_SIZE,
				recordCount * ipSize * 2 + (line + 1) * MAIN_RECORD_SIZE,
			),
		);
		const ccCode = numToCountryCode(arr[0] & 1023);
		return {
			latitude: (arr[0] >> 10) / 10000,
			longitude: arr[1] / 10000,
			country: ccCode,
		};
	}

	return null;
}
