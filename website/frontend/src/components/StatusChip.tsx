import { styled } from '@linaria/react'
import { forest, lime, success } from '../colors.js'
import { text } from '../fonts.js'
import { c } from '../utils.js'
import { css } from '@linaria/core'
import { RunStatus } from 'shared'
import { PropsWithChildren } from 'react'

type Style = 'bold' | 'minimal'

const colors: Record<RunStatus['type'], string> = {
	active: success[400],
	funding: lime[300],
	completed: forest[500],
}

const StatusChipContainer = styled.div`
	display: flex;
	align-items: center;
	justify-content: center;
	padding: 4px 8px;
	gap: 8px;
	text-transform: uppercase;
	color: ${(props) =>
		props.inverted ? 'var(--color-bg)' : 'var(--color-fg)'};
`

const Dot = styled.span`
	height: 1em;
	width: 1em;
	background-color: ${({ color }) => color};
	border-radius: 100%;
	display: inline-block;
`

const Bold = css`
	color: var(--color-bg);
	background: var(--color-fg);
`
const BoldInverted = css`
	color: var(--color-fg);
	background: var(--color-bg);
`

export function StatusChip({
	status,
	style,
	inverted,
	children
}: PropsWithChildren<{
	status: RunStatus['type']
	style: Style
	inverted?: boolean
}>) {
	return (
		<StatusChipContainer
			inverted={inverted}
			className={c(
				text['body/sm/medium'],
				style === 'bold' && (inverted ? BoldInverted : Bold),
			)}
		>
			<Dot color={colors[status]} />
			{children}
		</StatusChipContainer>
	)
}
