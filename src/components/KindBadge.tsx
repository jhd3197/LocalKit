import { KIND_DOCKER, KIND_PHP } from "../lib/types";
import { BrandIcon, kindIcon } from "../lib/brandIcons";

/**
 * A small pill with the real brand mark for a site's stack kind (plans 22,
 * 27). WordPress is the neutral default (zinc); Docker keeps its sky accent
 * and PHP gets indigo, so mixed lists scan at a glance.
 */
const META: Record<string, { label: string; title: string; className: string }> = {
  [KIND_DOCKER]: {
    label: "Docker",
    title: "Docker Compose project",
    className: "border-sky-800/60 bg-sky-950/40 text-sky-300",
  },
  [KIND_PHP]: {
    label: "PHP",
    title: "PHP / Laravel site",
    className: "border-indigo-800/60 bg-indigo-950/40 text-indigo-300",
  },
};

const WP_META = {
  label: "WP",
  title: "WordPress site",
  className: "border-zinc-700 bg-zinc-800/60 text-zinc-400",
};

export default function KindBadge({ kind }: { kind: string }) {
  const meta = META[kind] ?? WP_META;
  return (
    <span
      title={meta.title}
      className={`inline-flex shrink-0 items-center gap-1 rounded-full border px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-wide ${meta.className}`}
    >
      <BrandIcon icon={kindIcon(kind)} size={10} />
      {meta.label}
    </span>
  );
}
