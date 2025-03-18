import { styled } from '@linaria/react'
import { PropsWithChildren } from 'react'
import { text } from '../fonts.js'
import { forest, slate } from '../colors.js'

const Chit = styled.div`
	display: flex;
	flex-direction: column;
	align-items: center;
	justify-content: space-between;
`

const ChitValue = styled.span`
	background: var(--color-bg);
	padding: 2px 4px;

	.theme-dark & {
		background: ${forest[600]};
		color: ${slate[0]};
	}
`

export function InfoChit({
	label,
	children,
}: PropsWithChildren<{ label: string }>) {
	return (
		<Chit>
			<ChitValue className={text['body/sm/regular']}>
				{children}
			</ChitValue>
			<span className={text['aux/xs/regular']}>{label}</span>
		</Chit>
	)
}
