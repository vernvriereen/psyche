import { TwitterIcon } from "./icons/twitter";
import { BlueskyIcon } from "./icons/bluesky";
import { GithubIcon } from "./icons/github";

const icons = {
	bluesky: BlueskyIcon,
	github: GithubIcon,
	twitter: TwitterIcon,
} as const;

const links = {
	bluesky: "https://bsky.app/profile/nousresearch.com",
	github: "https://github.com/NousResearch/DisTrO",
	twitter: "https://x.com/nousresearch",
};

export function SocialIcon({
	type,
}: { type: "bluesky" | "github" | "twitter" }) {
	const Icon = icons[type];
	return (
		<a
			href={links[type]}
			className="bg-plain w-[16px] h-[16px] p-[2px] rounded-sm"
		>
			<Icon width={12} height={12} className="fill-backdrop" />
		</a>
	);
}
