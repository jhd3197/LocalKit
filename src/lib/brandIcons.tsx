// Real brand marks (WordPress, Docker, PHP…) via Iconify, bundled OFFLINE
// (plan 27 — pattern borrowed from Faro's plan 14).
//
// This is additive: lucide (components/icons.tsx) still owns UI chrome.
// Iconify fills the one gap it can't — recognizable brand logos on site
// kinds.
//
// Offline by construction: the `Icon` component comes from
// `@iconify/react/offline` (no API/fetch code — nothing can reach
// api.iconify.design) and is handed raw icon DATA from the committed,
// curated `brandIconData.ts`. A string name is never looked up over the
// network.

import { Icon } from "@iconify/react/offline";
import { KIND_DOCKER, KIND_PHP } from "./types";
import { BRAND_ICONS } from "./brandIconData";

// Site kind → simple-icons name. WordPress is the default kind, so it is
// also the fallback for any kind this frontend doesn't know yet.
const KIND_ICON: Record<string, string> = {
  [KIND_DOCKER]: "docker",
  [KIND_PHP]: "php",
};

/** The simple-icons name for a site kind's brand mark. */
export function kindIcon(kind: string): string {
  return KIND_ICON[kind] ?? "wordpress";
}

/** A thin, offline wrapper over Iconify's `<Icon>`. Renders curated icon DATA
 *  (never a network name); returns `null` for an unknown key so callers can
 *  fall back to a lucide glyph. simple-icons marks are monochrome and follow
 *  `currentColor`, so they tint like any other icon. */
export function BrandIcon({
  icon,
  size = 16,
  className,
  title,
}: {
  icon: string;
  size?: number;
  className?: string;
  /** When set, the icon is meaningful (adds a tooltip + drops aria-hidden). */
  title?: string;
}) {
  const data = BRAND_ICONS[icon];
  if (!data) return null;
  return (
    <Icon
      icon={data}
      width={size}
      height={size}
      className={className}
      role={title ? "img" : undefined}
      aria-label={title}
      aria-hidden={title ? undefined : true}
    />
  );
}
