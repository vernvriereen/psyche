import { styled } from '@linaria/react'
import { forest, lime } from '../colors.js'

const Container = styled.div`
	height: ${(props) => props.chunkHeight + 8}px;
	padding: 2px;
	background-color: var(--color-bg);
	border: 2px solid;
	border-color: rgba(0, 0, 0, 0.5) rgba(255, 255, 255, 0.5)
		rgba(255, 255, 255, 0.5) rgba(0, 0, 0, 0.5);

	& > div {
		height: 100%;
		background-size: ${(props) => props.chunkWidth + 4}px 16px;
		.theme-dark & {
			background-image: linear-gradient(
				to right,
				${(props) => props.big ? lime[300] : forest[500]} ${(props) => props.chunkWidth}px,
				transparent 4px
			);
		}
		.theme-light & {
			background-image: linear-gradient(
				to right,
				${forest[500]} ${(props) => props.chunkWidth}px,
				transparent 4px
			);
		}
	}
`
export function ProgressBar({
	ratio,
	chunkWidth,
	chunkHeight,
	big = false
}: {
	ratio: number
	chunkWidth: number
	chunkHeight: number
	big?: boolean
}) {
	return (
		<Container chunkWidth={chunkWidth} chunkHeight={chunkHeight} big={big}>
			<div style={{ width: `${ratio * 100}%` }}></div>
		</Container>
	)
}
