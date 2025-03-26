import { styled } from '@linaria/react'

const Container = styled.div`
display: inline-flex;`

const Collapsible = styled.div`
text-overflow: ellipsis;
flex-shrink: 1;
overflow: hidden;
white-space: nowrap;
`

export function Address({ children }: { children: string }) {
	return (
		<Container>
			{children.slice(0, 4)}
			<Collapsible>{children.slice(4, -4)}</Collapsible>
			{children.slice(-4)}
		</Container>
	)
}
