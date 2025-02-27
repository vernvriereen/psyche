import { styled } from '@linaria/react'

import PsycheLogo from '../assets/icons/psyche-box.svg?react'
import Symbol02 from '../assets/icons/symbol-02.svg?react'
import Symbol06 from '../assets/icons/symbol-06.svg?react'
import CornerFleur from '../assets/icons/corner-fleur.svg?react'
import Sun from '../assets/icons/sun.svg?react'
import Moon from '../assets/icons/moon.svg?react'
import { css } from '@linaria/core'
import { text } from '../fonts.js'
import React, { Children, PropsWithChildren } from 'react'
import { Button } from './Button.js'
import { c, svgFillCurrentColor } from '../utils.js'
import { useDarkMode } from 'usehooks-ts'
import { iconClass } from '../icon.js'

const smallBreakpoint = '872px'

const NavContainer = styled.div`
	display: flex;
	flex-direction: row;
	padding: 8px;
	background: var(--color-bg);
`

const NavMain = styled.div`
	display: grid;
	grid-template-columns: minmax(256px, 1fr) minmax(312px, 1fr) minmax(
			64px,
			1fr
		);

	& > .buttons {
		justify-self: end;
	}

	@media (width < ${smallBreakpoint}) {
		grid-template-columns: minmax(256px, 1fr) minmax(200px, 300px);
		padding-right: 128px;
		& > .buttons {
			justify-self: start;
			flex-direction: row;
		}
	}
	@media (width < 722px) {
		grid-template-columns: 300px;
	}
	gap: 24px;
	width: 100%;
	padding: 24px;
`

const VerticalStack = styled.div`
	display: flex;
	flex-direction: column;
	gap: 12px;
	width: fit-content;

	& > span {
		display: inline-flex;
		align-items: center;
	}
`

const psycheLogo = css`
	width: 1em;
	height: 1em;
	padding-right: 12px;
`

const symbol02 = css`
	padding-left: 8px;
	height: 1.5em;
	width: 2em;
`

const lineHeightSymbol = css`
	height: 1em;
	width: 1em;
`

const Chip = styled.div`
	background: var(--color-fg);
	color: var(--color-bg);
	width: fit-content;
	padding: 0 4px;
`

const Links = styled.div`
	display: flex;
	justify-content: space-between;
	align-items: center;

	& > a {
		color: var(--color-fg);
		text-transform: uppercase;
	}
`

function SymbolSeparatedItems({ children }: PropsWithChildren) {
	return (
		<Links>
			{Children.toArray(children).map((link, i) => (
				<React.Fragment key={i}>
					{i !== 0 && (
						<Symbol06
							className={c([lineHeightSymbol, iconClass])}
						/>
					)}
					{link}
				</React.Fragment>
			))}
		</Links>
	)
}
const cornerFleur = css`
	min-width: 128px;
	height: 128px;

	@media (width < ${smallBreakpoint}) {
		position: absolute;
		overflow: visible;
		top: 8px;
		right: 0;
	}

	@media (width < 380px) {
		width: 25vw;
		height: 25vw;
	}
`
export function Header() {
	const {
		isDarkMode,
		enable: enableDarkMode,
		disable: disableDarkMode,
	} = useDarkMode()
	return (
		<NavContainer>
			<NavMain>
				<VerticalStack>
					<span className={text['display/5xl']}>
						<PsycheLogo className={c([psycheLogo, iconClass])} />
						<span>nous psyche</span>
					</span>
					<span className={text['body/sm/medium']}>
						DISTRIBUTED INTELLIGENCE NETWORK
						<Symbol02 className={c([symbol02, iconClass])} />
					</span>
					<Chip className={text['aux/xs/semibold']}>TESTNET</Chip>
				</VerticalStack>
				<div className={text['body/sm/medium']}>
					<VerticalStack>
						<div>Cooperative training over&#8209;the&#8209;internet</div>
						<SymbolSeparatedItems>
						<a href="#">github</a>
							<a href="#">forum</a>
							<a href="#">docs</a>
							<a href="https://twitter.com/psycheoperation">
								Twitter
							</a>
							<a href="https://discord.gg/psychenetwork">
								Discord
							</a>
						</SymbolSeparatedItems>
					</VerticalStack>
				</div>
				<VerticalStack className="buttons">
					<Button
						style="theme"
						icon={{ side: 'left', svg: Sun }}
						pressed={!isDarkMode}
						onClick={disableDarkMode}
					>
						Light
					</Button>
					<Button
						style="theme"
						icon={{ side: 'left', svg: Moon }}
						pressed={isDarkMode}
						onClick={enableDarkMode}
					>
						Dark
					</Button>
				</VerticalStack>
			</NavMain>
			<CornerFleur className={`${svgFillCurrentColor} ${cornerFleur}`} />
		</NavContainer>
	)
}
