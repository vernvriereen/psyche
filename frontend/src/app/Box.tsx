import {
	Children,
	type PropsWithChildren,
	type ReactNode,
	cloneElement,
	isValidElement,
} from "react";

function giveLeafChildrenBgBackdrop(child: ReactNode | null): React.ReactNode {
	if (!isValidElement(child)) {
		return child;
	}

	const hasChildren =
		Children.count(child.props.children) > 0 &&
		Children.toArray(child.props.children).every((c) => isValidElement(c));

	if (!hasChildren) {
		return cloneElement(child, {
			className: `${child.props.className || ""} bg-backdrop px-1`.trim(),
			// biome-ignore lint/suspicious/noExplicitAny: idk lol
		} as any);
	}
	return cloneElement(child, {
		children: Children.map(child.props.children, giveLeafChildrenBgBackdrop),
		// biome-ignore lint/suspicious/noExplicitAny: idk lol
	} as any);
}

export function Box({
	children,
	title,
	fullH,
}: PropsWithChildren<{
	title: ReactNode;
	fullH?: boolean;
}>) {
	return (
		<div className="relative pt-4 pb-2 w-full">
			<div className="px-6 absolute -translate-y-[50%] w-full text-plain">
				{Children.map(<span>{title}</span>, giveLeafChildrenBgBackdrop)}
			</div>
			<div
				className={`border-2 border-primary rounded-md p-2 pt-4 pb-2 ${fullH ?? true ? "h-full" : ""}`}
			>
				{children}
			</div>
		</div>
	);
}
