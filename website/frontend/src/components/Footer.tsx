import { styled } from '@linaria/react'
import { SymbolSeparatedItems } from './SymbolSeparatedItems.js'
import { text } from '../fonts.js'
import IrohLogo from '../assets/logos/iroh.svg?react'
import TorchLogo from '../assets/logos/pytorch.svg?react'
import HfLogo from '../assets/logos/hf.svg?react'

const Container = styled.div`
	background: var(--color-bg);
	color: var(--color-fg);
	display: flex;
	justify-content: space-between;
	padding: 2px 16px;
	& a svg {
		height: 1em;
	}
	& path {
		fill: currentColor;
	}
`

const PoweredBy = styled.div`
	display: flex;
	justify-content: space-between;
	align-items: center;
	gap: 12px;

	& > a {
		color: var(--color-fg);
	}
`

export function Footer() {
	return (
		<Container className={text["aux/sm/medium"]}>
			<div>&copy;{new Date().getFullYear()} psyche foundation</div>
			<PoweredBy>
				<span>built on code by</span>
				<a href="https://www.iroh.computer/">
					<IrohLogo />
				</a>
				<a href="https://pytorch.org/">
					<TorchLogo />
				</a>
				<a href="https://huggingface.co/">
					<HfLogo />
				</a>
			</PoweredBy>
			<SymbolSeparatedItems>
				<a href="https://twitter.com/psycheoperation">twitter</a>
				<a href="https://discord.gg/psychenetwork">discord</a>
				<a href="https://nousresearch.com/nous-psyche/">blog post</a>
			</SymbolSeparatedItems>
		</Container>
	)
}
