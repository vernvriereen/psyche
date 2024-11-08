interface RGB {
	r: number;
	g: number;
	b: number;
}

interface Lab {
	l: number;
	a: number;
	b: number;
}

export function lerpColor(color1: RGB, color2: RGB, factor: number): RGB {
	// Ensure factor is between 0 and 1
	const clampedFactor = Math.max(0, Math.min(1, factor));

	// Convert RGB to Lab
	const lab1 = rgbToLab(color1);
	const lab2 = rgbToLab(color2);

	// Interpolate in Lab space
	const lerpedLab: Lab = {
		l: lab1.l + clampedFactor * (lab2.l - lab1.l),
		a: lab1.a + clampedFactor * (lab2.a - lab1.a),
		b: lab1.b + clampedFactor * (lab2.b - lab1.b),
	};

	// Convert back to RGB
	return labToRgb(lerpedLab);
}

function rgbToLab(rgb: RGB): Lab {
	// Convert RGB to XYZ
	let r = rgb.r / 255;
	let g = rgb.g / 255;
	let b = rgb.b / 255;

	r = r > 0.04045 ? ((r + 0.055) / 1.055) ** 2.4 : r / 12.92;
	g = g > 0.04045 ? ((g + 0.055) / 1.055) ** 2.4 : g / 12.92;
	b = b > 0.04045 ? ((b + 0.055) / 1.055) ** 2.4 : b / 12.92;

	const x = (r * 0.4124 + g * 0.3576 + b * 0.1805) / 0.95047;
	const y = (r * 0.2126 + g * 0.7152 + b * 0.0722) / 1.0;
	const z = (r * 0.0193 + g * 0.1192 + b * 0.9505) / 1.08883;

	// Convert XYZ to Lab
	const fx = x > 0.008856 ? x ** (1 / 3) : 7.787 * x + 16 / 116;
	const fy = y > 0.008856 ? y ** (1 / 3) : 7.787 * y + 16 / 116;
	const fz = z > 0.008856 ? z ** (1 / 3) : 7.787 * z + 16 / 116;

	return {
		l: 116 * fy - 16,
		a: 500 * (fx - fy),
		b: 200 * (fy - fz),
	};
}

function labToRgb(lab: Lab): RGB {
	// Convert Lab to XYZ
	const y = (lab.l + 16) / 116;
	const x = lab.a / 500 + y;
	const z = y - lab.b / 200;

	const x3 = x ** 3;
	const y3 = y ** 3;
	const z3 = z ** 3;

	let xr = x3 > 0.008856 ? x3 : (x - 16 / 116) / 7.787;
	let yr = y3 > 0.008856 ? y3 : (y - 16 / 116) / 7.787;
	let zr = z3 > 0.008856 ? z3 : (z - 16 / 116) / 7.787;

	xr *= 0.95047;
	yr *= 1.0;
	zr *= 1.08883;

	// Convert XYZ to RGB
	let r = xr * 3.2406 + yr * -1.5372 + zr * -0.4986;
	let g = xr * -0.9689 + yr * 1.8758 + zr * 0.0415;
	let b = xr * 0.0557 + yr * -0.204 + zr * 1.057;

	r = r > 0.0031308 ? 1.055 * r ** (1 / 2.4) - 0.055 : 12.92 * r;
	g = g > 0.0031308 ? 1.055 * g ** (1 / 2.4) - 0.055 : 12.92 * g;
	b = b > 0.0031308 ? 1.055 * b ** (1 / 2.4) - 0.055 : 12.92 * b;

	return {
		r: Math.max(0, Math.min(255, Math.round(r * 255))),
		g: Math.max(0, Math.min(255, Math.round(g * 255))),
		b: Math.max(0, Math.min(255, Math.round(b * 255))),
	};
}
