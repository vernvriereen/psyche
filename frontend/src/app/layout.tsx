import "./globals.css";

export const metadata = {
	title: "Nous DisTrO",
	description: "Nous DisTrO",
};

import { Ubuntu_Mono } from "next/font/google";

// If loading a variable font, you don't need to specify the font weight
const mono = Ubuntu_Mono({
	subsets: ["latin"],
	display: "swap",
	weight: ["700"],
	variable: "--font-ubuntu-mono",
});

export default function RootLayout({
	children,
}: {
	children: React.ReactNode;
}) {
	return (
		<html lang="en" className={`${mono.variable}`}>
			<body>{children}</body>
		</html>
	);
}
