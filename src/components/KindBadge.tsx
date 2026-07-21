import { KIND_DOCKER } from "../lib/types";

/**
 * A small pill marking a site's stack kind (plan 22). WordPress is the neutral
 * default (zinc); a Docker project gets a distinct sky accent so mixed lists
 * scan at a glance.
 */
export default function KindBadge({ kind }: { kind: string }) {
  const isDocker = kind === KIND_DOCKER;
  const cls = isDocker
    ? "border-sky-800/60 bg-sky-950/40 text-sky-300"
    : "border-zinc-700 bg-zinc-800/60 text-zinc-400";
  return (
    <span
      title={isDocker ? "Docker Compose project" : "WordPress site"}
      className={`inline-flex shrink-0 items-center rounded-full border px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-wide ${cls}`}
    >
      {isDocker ? "Docker" : "WP"}
    </span>
  );
}
