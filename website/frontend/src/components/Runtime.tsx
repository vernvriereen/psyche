import { styled } from '@linaria/react'
import { useState } from 'react'
import { useInterval } from 'usehooks-ts'

const Number = styled.span`
	color: var(--color-fg);
	background: var(--color-bg);
	padding: 4px;
`

const Container = styled.span`
	padding: 4px;
`

export function Runtime({ start, end }: { start: Date; end?: Date }) {
	const [now, setNow] = useState(Date.now())
	useInterval(() => setNow(Date.now()), 1000)

	const elapsed = (end ? end.valueOf() : now) - start.valueOf()
	const days = Math.floor(elapsed / (1000 * 60 * 60 * 24))
	const hours = Math.floor(
		(elapsed % (1000 * 60 * 60 * 24)) / (1000 * 60 * 60)
	)
	const minutes = Math.floor((elapsed % (1000 * 60 * 60)) / (1000 * 60))
	const seconds = Math.floor((elapsed % (1000 * 60)) / 1000)
	return (
		<Container>
			<Number>{days}d</Number>:<Number>{hours}h</Number>:
			<Number>{minutes}m</Number>:<Number>{seconds}s</Number>
		</Container>
	)
}
