/**
 * Deterministic monogram tile — a site's face (plan 27).
 *
 * The hue is hashed from the *slug* (not the display name), so it survives
 * renames-in-place, and is identical in grid, list and detail views. The tile
 * doubles as a status light: up sites render saturated with a same-hue glow,
 * everything else keeps the hue but dimmed — identity persists while the
 * site sleeps.
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
  size = "md",
}: {
  name: string;
  slug: string;
  status: string;
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
        background: `hsl(${hue} 26% 17%)`,
        boxShadow: "inset 0 1px 0 hsl(0 0% 100% / 0.05)",
        color: `hsl(${hue} 42% 70%)`,
      };
  return (
    <span
      aria-hidden
      style={style}
      className={`flex shrink-0 select-none items-center justify-center font-semibold tracking-wide ${
        SIZES[size]
      } ${status === "creating" ? "animate-pulse" : ""}`}
    >
      {initials(name)}
    </span>
  );
}
