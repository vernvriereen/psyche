import { createFileRoute, Outlet } from '@tanstack/react-router'
import { Header } from '../../components/Header.js'
import { styled } from '@linaria/react'
import { OutlineBox } from '../../components/OutlineBox.js'
import { ContributeCompute } from '../../components/ContributeCompute.js'
import { fetchContributions } from '../../fetchRuns.js'
import { Footer } from '../../components/Footer.js'

export const Route = createFileRoute('/runs')({
	loader: fetchContributions,
	component: RouteComponent,
})

const Main = styled.div`
	background: var(--bg-svg);
	background-size: 12px;
	display: flex;
	flex-direction: column;
	container-type: inline-size;
	align-items: space-around;
	justify-content: space-between;
	min-height: 100vh;
`

const MainContainer = styled.div`
	display: flex;
`
const MainContents = styled.div`
	flex-basis: 1700px;
	margin: 0 auto;
	display: grid;
	grid-template-columns: 512px 1fr;
	flex-wrap: wrap;
	justify-content: center;
	gap: 36px;
	padding: 36px;
	@container (width < calc(1024px + (36px * 2))) {
		grid-template-columns: 1fr;
		padding: 16px;
	}
	@container (width < 400px) {
		padding: 4px;
	}
	& > * {
		background: var(--color-bg);
	}
`
function RouteComponent() {
	const contributions = Route.useLoaderData()
	return (
		<Main>
			<Header />
			<MainContainer>
				<MainContents>
					<OutlineBox title="mining pool">
						<ContributeCompute {...contributions} />
					</OutlineBox>
					<OutlineBox title="training">
						<Outlet />
					</OutlineBox>
				</MainContents>
			</MainContainer>
			<Footer />
		</Main>
	)
}
