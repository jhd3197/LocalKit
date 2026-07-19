import type { RouterStatus } from "./types";

/** TLD for local domains (kept in sync with `router::TLD` in the backend). */
export const DOMAIN_TLD = "test";

/**
 * The URL a site should be opened/displayed at: its `*.test` domain when
 * the router is enabled and running (https once the CA is trusted), otherwise
 * plain `http://localhost:<port>`.
 */
export function siteUrl(slug: string, port: number, router: RouterStatus | null): string {
  if (router?.enabled && router.running) {
    return `${router.ca_trusted ? "https" : "http"}://${slug}.${DOMAIN_TLD}`;
  }
  return `http://localhost:${port}`;
}
