import makeRegl, { DefaultContext } from 'regl'
import { makeIcosphere } from './icosphere.js'

import { mat4, mat3, vec3 } from 'gl-matrix'
const icosphere = makeIcosphere(6)

import vert from './vert.glsl'
import frag from './frag.glsl'

interface Uniforms {
	mvp: mat4
	model: mat4
	eye: vec3
	time: number
	normal: mat3
	ditherColor: vec3
}

interface Props {
	eye: vec3
	target: vec3
}

interface CustomContext {
	proj: mat4
	model: mat4
	view: mat4
	time: number
	ditherColor: vec3
}

function color(colorString: string): vec3 {
	const { style } = new Option()
	style.color = colorString
	const rgb = style.color
		.split('(')[1]
		.split(')')[0]
		.split(',')
		.map(Number)
		.map((n) => n / 255) as vec3
	return rgb
}

export function createSphereAnimation(
	canvas: HTMLCanvasElement,
	ditherColor: string
) {
	const regl = makeRegl({
		canvas: canvas,
		attributes: {
			premultipliedAlpha: false,
		},
	})
	const camera = regl<Uniforms, {}, Props, CustomContext>({
		context: {
			proj: () =>
				mat4.perspectiveNO(mat4.create(), Math.PI / 2, 1, 0.01, 10),
			model: () => mat4.create(),
			view: (_, { eye, target }: Props) =>
				mat4.lookAt(
					mat4.create(),
					eye,
					target,
					[0, 1, 0] // up
				),
			time: ({ time }: DefaultContext) => time,
			ditherColor: color(ditherColor),
		},
		uniforms: {
			mvp: ({ proj, view, model }) => {
				const viewProj = mat4.multiply(mat4.create(), proj, view)
				return mat4.multiply(mat4.create(), viewProj, model)
			},
			model: ({ model }) => model,
			eye: (_, { eye }) => eye,
			time: ({ time }) => time,
			normal: ({ model }) => {
				const invertedModel = mat4.invert(mat4.create(), model)
				const transposedInvertedModel = mat4.transpose(
					mat4.create(),
					invertedModel
				)
				return mat3.fromMat4(mat3.create(), transposedInvertedModel)
			},
			ditherColor: ({ ditherColor }) => ditherColor,
		},
	})

	const drawSphere = regl({
		vert,
		frag,
		attributes: {
			position: icosphere.positions,
		},
		elements: icosphere.cells,
		uniforms: {
			lightDir: [1, 1, 0.3],
		},
	})

	return regl.frame(() => {
		try {
			camera(
				{
					eye: [0, 0, 1.4],
					target: [0, 0, 0],
				},
				() => {
					regl.clear({ color: [0, 0, 0, 0] })
					drawSphere()
				}
			)
		} catch (e) {
			regl.destroy()
		}
	})
}
