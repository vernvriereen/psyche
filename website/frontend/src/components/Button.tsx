import { ButtonHTMLAttributes, FunctionComponent } from 'react'
import { text } from '../fonts.js'
import { css, LinariaClassName } from '@linaria/core'
import { forest, gold, slate } from '../colors.js'
import { Link, LinkProps } from '@tanstack/react-router'
import { iconClass } from '../icon.js'
import { svgFillCurrentColor } from '../utils.js'

const buttonStyle = css`
	text-decoration: none;

	padding: 0;
	display: inline-flex;
	align-items: center;
	justify-content: flex-start;

	border: none;
	cursor: pointer;
	text-transform: uppercase;

	box-shadow:
		inset -1px -1px 0px rgba(0, 0, 0, 0.5),
		inset 1px 1px 0px rgba(255, 255, 255, 0.5);

	&:disabled {
		cursor: not-allowed;
	}

	&:not(:disabled):active,
	&.fakePressed {
		box-shadow:
			inset -1px -1px 0px rgba(255, 255, 255, 0.5),
			inset 1px 1px 0px rgba(0, 0, 0, 0.5);
	}

	& div.icon {
		display: flex;
		align-self: stretch;
		align-items: center;
		justify-content: center;
	}

	& .icon svg {
		margin: 0px 3px;
		width: 0.9em;
		height: 0.9em;
	}

	& .contents {
		flex-grow: 1;
		padding: 0 4px;
	}
	&:has(.left) .contents {
		padding-left: 0;
	}
	&:has(.right) .contents {
		padding-right: 0;
	}
`

const buttonPrimaryStyle = css`
	.theme-light & {
		background: ${gold[300]};
		color: ${slate[1000]};

		&:disabled {
			background: ${slate[300]};
			color: ${slate[400]};
			text-shadow: 1px 1px 0px ${slate[200]};
		}
	}

	.theme-dark & {
		background: ${gold[300]};
		color: ${slate[1000]};

		&:disabled {
			background: ${forest[600]};
			color: ${forest[700]};
			text-shadow: 1px 1px 0px ${forest[500]};
		}
	}
`

const buttonSecondaryStyle = css`
	.theme-light & {
		background: ${slate[300]};
		color: ${slate[1000]};

		&:disabled {
			background: ${slate[200]};
			color: ${slate[300]};
			text-shadow: 1px 1px 0px ${slate[100]};
		}
	}

	.theme-dark & {
		background: ${forest[500]};
		color: ${slate[0]};

		&:disabled {
			background: ${forest[600]};
			color: ${forest[700]};
			text-shadow: 1px 1px 0px ${forest[500]};
		}
	}
`

const buttonThemeStyle = css`
	.theme-light & {
		background: ${slate[200]};
		color: ${slate[1000]};

		&:disabled {
			background: ${slate[200]};
			color: ${slate[300]};
			text-shadow: 1px 1px 0px ${slate[100]};
		}

		&:not(:disabled):active,
		&.fakePressed {
			background: ${slate[100]};
		}
	}

	.theme-dark & {
		background: ${forest[700]};
		color: ${slate[0]};

		&:disabled {
			background: ${forest[600]};
			color: ${forest[700]};
			text-shadow: 1px 1px 0px ${forest[500]};
		}

		&:not(:disabled):active,
		&.fakePressed {
			background: ${forest[600]};
		}
	}
`

const buttonActionStyle = css`
	border: 2px solid;
	background: transparent;
	border-color: var(--button-color);
	color: var(--button-color);

	.theme-light & {
		--button-color: ${forest[700]};
		--button-bg: ${slate[0]};

		&:not(:disabled):not(:active):not(.fakePressed):hover {
			--button-color: ${forest[400]};
		}

		&:disabled {
			--button-color: ${slate[300]};
		}

		&:not(:disabled):active,
		&.fakePressed {
			background: ${forest[700]};
			color: ${slate[0]};
			border-color: ${forest[700]};
		}
	}

	.theme-dark & {
		--button-color: ${slate[0]};
		--button-bg: ${forest[700]};

		&:not(:disabled):not(:active):not(.fakePressed):hover {
			--button-color: ${slate[400]};
		}

		&:disabled {
			--button-color: ${forest[600]};
		}

		&:not(:disabled):active,
		&.fakePressed {
			background: ${slate[0]};
			color: ${forest[700]};
			border-color: ${slate[0]};
		}
	}

	& div.icon {
		background: var(--button-color);
		color: var(--button-bg);
	}

	&,
	&:not(:disabled):active,
	&.fakePressed {
		box-shadow: none;
	}
	&:has(.left) .contents {
		padding-left: 4px;
	}
	&:has(.right) .contents {
		padding-right: 4px;
	}
`

interface ButtonProps {
	style: 'primary' | 'secondary' | 'theme' | 'action'
	icon?: {
		svg: FunctionComponent<React.SVGProps<SVGSVGElement>>
		side: 'left' | 'right'
	}
	pressed?: boolean
}

const styles: Record<ButtonProps['style'], LinariaClassName> = {
	primary: buttonPrimaryStyle,
	secondary: buttonSecondaryStyle,
	theme: buttonThemeStyle,
	action: buttonActionStyle,
}

type HTMLButtonProps = Omit<ButtonHTMLAttributes<HTMLButtonElement>, 'style'>

export function Button<T extends LinkProps | HTMLButtonProps>(
	props: T & ButtonProps
) {
	const { style, pressed } = props
	const className = `${text['button/sm']} ${buttonStyle} ${styles[style]} ${
		pressed ? 'fakePressed' : ''
	} ${'className' in props ? props.className : ''}`

	if ('to' in props) {
		const {
			style: _,
			children,
			icon,
			pressed: __,
			...buttonProps
		} = props as LinkProps & ButtonProps
		return (
			<Link {...buttonProps} className={className}>
				{({ isActive, isTransitioning }) => (
					<>
						{icon?.side === 'left' && (
							<div className="icon">
								<icon.svg className={`${svgFillCurrentColor} left`} />
							</div>
						)}
						{children && (
							<span className="contents">
								{typeof children === 'function'
									? children({ isActive, isTransitioning })
									: children}
							</span>
						)}
						{icon?.side === 'right' && (
							<div className="icon">
								<icon.svg className={`${svgFillCurrentColor} right`} />
							</div>
						)}
					</>
				)}
			</Link>
		)
	} else {
		const {
			style: _,
			children,
			icon,
			pressed: __,
			...buttonProps
		} = props as HTMLButtonProps & ButtonProps
		return (
			<button {...buttonProps} className={className}>
				{icon?.side === 'left' && (
					<div className="icon">
						<icon.svg className={`${iconClass} left`} />
					</div>
				)}
				{children && <span className="contents">{children}</span>}
				{icon?.side === 'right' && (
					<div className="icon">
						<icon.svg className={`${iconClass} right`} />
					</div>
				)}
			</button>
		)
	}
}
