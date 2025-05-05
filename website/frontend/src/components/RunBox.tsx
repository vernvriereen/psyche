import { styled } from '@linaria/react'
import { ReactNode } from '@tanstack/react-router'
import { PropsWithChildren } from 'react'
import { forest, slate } from '../colors.js'

const RunHeader = styled.div`
	display: flex;
	align-items: center;
	justify-content: space-between;
	border-bottom: 2px solid;

	.theme-light & {
		color: ${forest[700]};
		border-color: ${slate[500]};
	}
	.theme-dark & {
		color: ${forest[300]};
		border-color: ${forest[500]};
	}
`

const Box = styled.div`
	& > * {
		padding: 8px 16px;
	}

	margin-top: 24px;
	margin-bottom: 24px;
	border: 2px solid;

	display: flex;
	flex-direction: column;

	position: relative;

	.theme-light & {
		border-color: ${slate[500]};
	}
	.theme-dark & {
		border-color: ${forest[500]};
	}
`
export function RunBox({
	children,
	title,
	titleClass,
}: PropsWithChildren<{ title: ReactNode; titleClass?: string }>) {
	return (
		<Box>
			<RunHeader className={titleClass}>{title}</RunHeader>
			{children}
		</Box>
	)
}
