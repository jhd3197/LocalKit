import type { RouterStatus, Site } from "./types";

/** TLD for local domains (kept in sync with `router::TLD` in the backend). */
export const DOMAIN_TLD = "test";

/**
 * The host port a site is actually reachable at — a docker project's own
 * published `app_port` when it has one, otherwise the reserved site port
 * (WordPress). Mirrors `SiteConfig::upstream_port` on the backend (plan 22).
 */
export function sitePort(site: Pick<Site, "port" | "config">): number {
  return site.config?.app_port ?? site.port;
}

/** Default router host ports — mirrors `router::DEFAULT_*_PORT`. */
export const DEFAULT_HTTP_PORT = 80;
export const DEFAULT_HTTPS_PORT = 443;
/** Suggested fallback pair when another program owns 80/443 (plan 16). */
export const FALLBACK_HTTP_PORT = 8080;
export const FALLBACK_HTTPS_PORT = 8443;

/** Is the router on the clean-URL ports? Mirrors `RouterPorts::is_default`. */
export function isDefaultPorts(router: RouterStatus | null): boolean {
  return (
    (router?.http_port ?? DEFAULT_HTTP_PORT) === DEFAULT_HTTP_PORT &&
    (router?.https_port ?? DEFAULT_HTTPS_PORT) === DEFAULT_HTTPS_PORT
  );
}

/**
 * The URL a site should be opened/displayed at: its `*.test` domain when
 * the router is enabled and running (https once the CA is trusted), otherwise
 * plain `http://localhost:<port>`.
 *
 * Mirror of `router::site_url` / `site_public_url` — in fallback mode the port
 * is spelled out and the scheme stays http, because a non-standard https port
 * would prompt for a second certificate exception even with the CA trusted.
 */
export function siteUrl(slug: string, port: number, router: RouterStatus | null): string {
  if (router?.enabled && router.running) {
    if (!isDefaultPorts(router)) {
      return `http://${slug}.${DOMAIN_TLD}:${router.http_port}`;
    }
    return `${router.ca_trusted ? "https" : "http"}://${slug}.${DOMAIN_TLD}`;
  }
  return `http://localhost:${port}`;
}
