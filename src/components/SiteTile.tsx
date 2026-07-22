import { BrandIcon, kindIcon } from "../lib/brandIcons";

/**
 * Deterministic monogram tile — a site's face (plans 27/28).
 *
 * The hue is hashed from the *slug* (not the display name), so it survives
 * renames-in-place, and is identical in the rail, grid, list and detail
 * views. The tile doubles as a status light: up sites render saturated with
 * a same-hue glow, everything else keeps the hue but dimmed — identity
 * persists while the site sleeps. Dimmed lightness comes from the theme
 * token layer, so tiles read correctly in light mode too.
 *
 * Pass `kind` to stamp the real brand mark (WordPress / Docker / PHP) on
 * the corner — the Faro pattern: the monogram stays the identity, the
 * stamp adds recognizability.
 */

const SIZES = {
  sm: "h-7 w-7 rounded-lg text-[10px]",
  md: "h-10 w-10 rounded-xl text-sm",
  lg: "h-12 w-12 rounded-xl text-base",
} as const;

/** FNV-1a, folded to a hue. Tiny, stable, decent spread on short slugs. */
export function hueFromSlug(slug: string): number {
  let h = 0x811c9dc5;
  for (let i = 0; i < slug.length; i++) {
    h ^= slug.charCodeAt(i);
    h = Math.imul(h, 0x01000193);
  }
  return (h >>> 0) % 360;
}

export function initials(name: string): string {
  const words = name.trim().split(/\s+/).filter(Boolean);
  if (words.length === 0) return "?";
  if (words.length === 1) return words[0].slice(0, 2).toUpperCase();
  return (words[0][0] + words[1][0]).toUpperCase();
}

export default function SiteTile({
  name,
  slug,
  status,
  kind,
  size = "md",
}: {
  name: string;
  slug: string;
  status: string;
  /** Site kind — when set, the brand mark is stamped on the corner. */
  kind?: string;
  size?: keyof typeof SIZES;
}) {
  const hue = hueFromSlug(slug);
  const up = status === "running" || status === "degraded";
  const style = up
    ? {
        background: `linear-gradient(135deg, hsl(${hue} 70% 58%), hsl(${hue} 72% 42%))`,
        boxShadow: `0 0 18px hsl(${hue} 72% 50% / 0.28), inset 0 1px 0 hsl(0 0% 100% / 0.25)`,
        color: "#fff",
      }
    : {
        background: `hsl(${hue} 26% var(--tile-dim-bg-l))`,
        boxShadow: "inset 0 1px 0 hsl(0 0% 100% / 0.05)",
        color: `hsl(${hue} 42% var(--tile-dim-fg-l))`,
      };
  return (
    <span
      aria-hidden
      style={style}
      className={`relative flex shrink-0 select-none items-center justify-center font-semibold tracking-wide ${
        SIZES[size]
      } ${status === "creating" ? "animate-pulse" : ""}`}
    >
      {initials(name)}
      {kind && size !== "sm" && (
        <span className="absolute -bottom-1 -right-1 flex h-[18px] w-[18px] items-center justify-center rounded-md bg-zinc-950 text-zinc-300 ring-1 ring-zinc-700">
          <BrandIcon icon={kindIcon(kind)} size={11} />
        </span>
      )}
    </span>
  );
}
