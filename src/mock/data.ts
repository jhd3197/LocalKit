// Fictional data for the mock build (`vite --mode mock`). Every hostname,
// credential and path below is made up — these exist so the UI renders
// populated for screenshots and manual previews, with no Docker or Tauri.
import type {
  AppInfo,
  Blueprint,
  Capabilities,
  RemoteWpSite,
  RouterStatus,
  ServerKitConnection,
  SiteDetail,
  SiteWithStatus,
  Snapshot,
  SyncRecord,
  WpInfo,
} from "../lib/types";

/** Capability matrices (plan 22) — mirror `site::Capabilities` in the backend. */
export const WP_CAPS: Capabilities = {
  domains: true,
  terminal: true,
  logs: true,
  snapshots: true,
  db_gui: true,
  db_sync: true,
  code_sync: true,
  one_click_login: true,
  wp_tools: true,
  search_replace: true,
};
export const DOCKER_CAPS: Capabilities = {
  domains: true,
  terminal: true,
  logs: true,
  snapshots: true,
  db_gui: false,
  db_sync: false,
  code_sync: true,
  one_click_login: false,
  wp_tools: false,
  search_replace: false,
};

// Keep in sync with WP_VERSIONS / PHP_VERSIONS in src-tauri/src/site.rs.
export const appInfo: AppInfo = {
  data_dir: "C:\\Users\\demo\\AppData\\Roaming\\localkit",
  sites_dir: "C:\\Users\\demo\\AppData\\Roaming\\localkit\\sites",
  wp_versions: ["6.7", "6.6", "6.5"],
  php_versions: ["8.3", "8.2", "8.1"],
  kinds: [
    { kind: "wordpress", capabilities: WP_CAPS },
    { kind: "docker", capabilities: DOCKER_CAPS },
  ],
};

/** The WordPress kind fields every WP mock site carries (plan 22). */
const WP_KIND = {
  kind: "wordpress",
  config: { service: "wordpress", sync_path: "wp-content" },
  capabilities: WP_CAPS,
} as const;

interface MockSite extends SiteWithStatus {
  db_password: string;
}

export const sites: MockSite[] = [
  {
    id: "site-pixel-bakery",
    name: "Pixel Bakery",
    slug: "pixel-bakery",
    path: "C:\\Users\\demo\\AppData\\Roaming\\localkit\\sites\\pixel-bakery",
    port: 8081,
    wp_version: "6.7",
    php_version: "8.3",
    status: "running",
    live_status: "running",
    admin_user: "admin",
    admin_pass: "cr0iss4nt-velvet-42",
    created_at: "2026-07-02T09:14:00Z",
    db_password: "m4ri4-pix3l-9917",
    connection_id: null,
    remote_site_id: null,
    ...WP_KIND,
  },
  {
    id: "site-acme-corporate",
    name: "Acme Corporate",
    slug: "acme-corporate",
    path: "C:\\Users\\demo\\AppData\\Roaming\\localkit\\sites\\acme-corporate",
    port: 8082,
    wp_version: "6.5",
    php_version: "8.1",
    status: "running",
    live_status: "running",
    admin_user: "admin",
    admin_pass: "acme-roadrunner-88",
    created_at: "2026-06-18T15:40:00Z",
    db_password: "m4ri4-acm3-5542",
    // Imported from the Production connection (plan 18) — drives the link
    // badge on the dashboard.
    connection_id: "conn-prod",
    remote_site_id: 12,
    ...WP_KIND,
  },
  {
    id: "site-hiking-blog",
    name: "Hiking Blog",
    slug: "hiking-blog",
    path: "C:\\Users\\demo\\AppData\\Roaming\\localkit\\sites\\hiking-blog",
    port: 8083,
    wp_version: "6.6",
    php_version: "8.2",
    status: "stopped",
    live_status: "stopped",
    admin_user: "admin",
    admin_pass: "summit-trail-2026",
    created_at: "2026-05-30T11:02:00Z",
    db_password: "m4ri4-h1k3-3308",
    connection_id: null,
    remote_site_id: null,
    ...WP_KIND,
  },
  {
    id: "site-client-demo",
    name: "Client Demo",
    slug: "client-demo",
    path: "C:\\Users\\demo\\AppData\\Roaming\\localkit\\sites\\client-demo",
    port: 8084,
    wp_version: "6.7",
    php_version: "8.3",
    status: "creating",
    live_status: "creating",
    admin_user: "admin",
    admin_pass: "d3mo-spr1ng-7741",
    created_at: "2026-07-19T01:58:00Z",
    db_password: "m4ri4-d3m0-1120",
    connection_id: null,
    remote_site_id: null,
    ...WP_KIND,
  },
  // A generic Docker app (plan 22) — proves the capability gating: no WP Admin,
  // no credentials/database panels, no clone/blueprint/push, just the generic
  // lifecycle, logs, terminal, a `.test` domain, and code snapshots.
  {
    id: "site-analytics-api",
    name: "Analytics API",
    slug: "analytics-api",
    path: "C:\\Users\\demo\\AppData\\Roaming\\localkit\\sites\\analytics-api",
    port: 8085,
    wp_version: "",
    php_version: "",
    status: "running",
    live_status: "running",
    admin_user: "",
    admin_pass: "",
    created_at: "2026-07-14T16:20:00Z",
    db_password: "",
    connection_id: null,
    remote_site_id: null,
    kind: "docker",
    config: { service: "api", sync_path: ".", app_port: 4000, db_engine: "postgres", db_service: "db" },
    capabilities: DOCKER_CAPS,
  },
];

export function siteDetail(site: MockSite): SiteDetail {
  return {
    ...site,
    db_host: "127.0.0.1",
    db_port: site.port + 10000,
    db_name: "wordpress",
    db_user: "wordpress",
  };
}

export const wpInfo: Record<string, WpInfo> = {
  "site-pixel-bakery": {
    core_version: "6.7.2",
    plugins: [
      { name: "akismet", status: "active", version: "5.3.5" },
      { name: "contact-form-7", status: "active", version: "6.0.3" },
      { name: "woocommerce", status: "active", version: "9.6.0" },
      { name: "hello-dolly", status: "inactive", version: "1.7.2" },
    ],
  },
  "site-acme-corporate": {
    core_version: "6.5.7",
    plugins: [
      { name: "advanced-custom-fields", status: "active", version: "6.3.12" },
      { name: "wordfence", status: "active", version: "8.0.3" },
      { name: "akismet", status: "inactive", version: "5.3.5" },
    ],
  },
};

export const siteLogs: Record<string, string> = {
  "site-pixel-bakery": [
    'wordpress-1  | [Sat Jul 19 01:12:04.338221 2026] [mpm_prefork:notice] [pid 1] AH00163: Apache/2.4.62 (Debian) PHP/8.3.14 configured -- resuming normal operations',
    "wordpress-1  | [Sat Jul 19 01:12:04.338264 2026] [core:notice] [pid 1] AH00094: Command line: 'apache2 -D FOREGROUND'",
    'wordpress-1  | 172.18.0.1 - - [19/Jul/2026:01:12:41 +0000] "GET / HTTP/1.1" 200 12453',
    'wordpress-1  | 172.18.0.1 - - [19/Jul/2026:01:12:42 +0000] "GET /wp-content/themes/pixel-bakery/style.css HTTP/1.1" 200 8211',
    'wordpress-1  | 172.18.0.1 - - [19/Jul/2026:01:13:02 +0000] "POST /wp-admin/admin-ajax.php HTTP/1.1" 200 148',
    'db-1         | 2026-07-19  1:12:05 0 [Note] mariadbd: ready for connections.',
    "db-1         | Version: '11.4.4-MariaDB'  socket: '/run/mysqld/mysqld.sock'  port: 3306",
  ].join("\n"),
  "site-acme-corporate": [
    'wordpress-1  | [Fri Jul 18 22:04:11.109384 2026] [mpm_prefork:notice] [pid 1] AH00163: Apache/2.4.62 (Debian) PHP/8.1.31 configured -- resuming normal operations',
    'wordpress-1  | 172.18.0.1 - - [18/Jul/2026:22:05:55 +0000] "GET /wp-login.php HTTP/1.1" 200 3120',
    'db-1         | 2026-07-18 22:04:12 0 [Note] mariadbd: ready for connections.',
  ].join("\n"),
  "site-hiking-blog": "No logs — containers are stopped.",
  "site-client-demo": "Creating containers…",
  "site-analytics-api": [
    'api-1  | {"level":"info","msg":"listening on :4000","ts":"2026-07-14T16:20:41Z"}',
    'api-1  | {"level":"info","msg":"connected to postgres","ts":"2026-07-14T16:20:42Z"}',
    'db-1   | 2026-07-14 16:20:40 UTC [1] LOG:  database system is ready to accept connections',
  ].join("\n"),
};

// M6 local domains: fictional router state. Enabled + running so dashboard /
// detail shots render `*.test` URLs; CA untrusted until trust_router_ca.
export const routerStatus: RouterStatus = {
  enabled: true,
  running: true,
  ca_trusted: false,
  error: null,
  conflicts: [],
  http_port: 80,
  https_port: 443,
};

/**
 * Plan 16: a fictional LocalWP-style router holding 80/443, so the conflict UX
 * is exercisable in mock mode. The router starts *already running*, so the
 * default screenshots are unaffected — toggle domains off then on to hit it,
 * and "Use fallback ports" resolves it (8080/8443 are free here).
 */
export const heldPorts: Record<number, string> = { 80: "httpd.exe", 443: "httpd.exe" };

/** In-memory app_settings KV (e.g. run_in_background for the tray toggle). */
export const appSettings: Record<string, string> = {};

export const connections: ServerKitConnection[] = [
  {
    id: "conn-prod",
    label: "Production",
    url: "https://panel.acme-hosting.example",
    api_key: "sk_live_demo_a94f8c2e71b34d0f",
    created_at: "2026-07-05T10:22:00Z",
  },
];

export const remoteSites: Record<string, RemoteWpSite[]> = {
  "conn-prod": [
    {
      id: 12,
      name: "acme-corporate",
      url: "https://acme-corporate.example",
      status: "running",
      wp_version: "6.5",
      php_version: "8.1",
      multisite: false,
      environment_count: 2,
    },
    {
      id: 27,
      name: "pixel-bakery",
      url: "https://pixelbakery.example",
      status: "running",
      wp_version: "6.7",
      php_version: "8.3",
      multisite: false,
      environment_count: 3,
    },
    {
      id: 31,
      name: "landing-tests",
      url: null,
      status: "stopped",
      wp_version: "6.6",
      php_version: "8.2",
      multisite: false,
      environment_count: 1,
    },
    // Exercises the two Import-blocked states: a multisite (never importable)
    // and a version pair with no exact local image (importable, with a warning).
    {
      id: 44,
      name: "agency-network",
      url: "https://network.agency.example",
      status: "running",
      wp_version: "6.7",
      php_version: "8.2",
      multisite: true,
      environment_count: 0,
    },
    {
      id: 51,
      name: "legacy-shop",
      url: "https://legacy-shop.example",
      status: "running",
      wp_version: "6.2",
      php_version: "7.4",
      multisite: false,
      environment_count: 0,
    },
  ],
};

/**
 * Plan 17 snapshots. Pixel Bakery shows the full mix — a manual one plus the
 * automatic ones push/pull/delete leave behind — so the kind badges and the
 * retention story are visible without Docker. Hiking Blog has none, which is
 * the empty state.
 */
export const snapshots: Record<string, Snapshot[]> = {
  "site-pixel-bakery": [
    {
      id: "20260719-084500-120",
      site_id: "site-pixel-bakery",
      site_name: "Pixel Bakery",
      site_slug: "pixel-bakery",
      created_at: "2026-07-19T08:45:00Z",
      kind: "pre_pull",
      note: "Production (#27 on https://panel.acme-hosting.example)",
      db_bytes: 4_312_770,
      code_bytes: 224_512_900,
      wp_version: "6.7",
    },
    {
      id: "20260718-142200-880",
      site_id: "site-pixel-bakery",
      site_name: "Pixel Bakery",
      site_slug: "pixel-bakery",
      created_at: "2026-07-18T14:22:00Z",
      kind: "manual",
      note: "before the checkout rewrite",
      db_bytes: 4_298_115,
      code_bytes: 223_998_042,
      wp_version: "6.7",
    },
    {
      id: "20260715-135500-410",
      site_id: "site-pixel-bakery",
      site_name: "Pixel Bakery",
      site_slug: "pixel-bakery",
      created_at: "2026-07-15T13:55:00Z",
      kind: "pre_push",
      note: "Production (#27 on https://panel.acme-hosting.example)",
      db_bytes: 4_105_663,
      code_bytes: 219_774_301,
      wp_version: "6.7",
    },
  ],
  "site-acme-corporate": [
    {
      id: "20260716-101200-005",
      site_id: "site-acme-corporate",
      site_name: "Acme Corporate",
      site_slug: "acme-corporate",
      created_at: "2026-07-16T10:12:00Z",
      kind: "manual",
      note: "",
      db_bytes: 1_884_204,
      code_bytes: 48_220_118,
      wp_version: "6.5",
    },
  ],
  // A docker app's snapshot is code-only — the DB dump is empty (db_bytes 0).
  "site-analytics-api": [
    {
      id: "20260714-170000-000",
      site_id: "site-analytics-api",
      site_name: "Analytics API",
      site_slug: "analytics-api",
      created_at: "2026-07-14T17:00:00Z",
      kind: "manual",
      note: "before the schema change",
      db_bytes: 0,
      code_bytes: 3_204_880,
      wp_version: "",
    },
  ],
};

/**
 * Plan 20 blueprints: reusable site recipes, so the NewSiteDialog "From
 * blueprint" section and its plugin/theme chips are reviewable without Docker.
 */
export const blueprints: Blueprint[] = [
  {
    id: "starter-shop",
    name: "Starter Shop",
    description: "WooCommerce storefront with our base theme and a set of starter products.",
    wp_version: "6.7",
    php_version: "8.3",
    plugins: [
      { name: "woocommerce", status: "active", version: "9.6.0" },
      { name: "contact-form-7", status: "active", version: "6.0.3" },
      { name: "akismet", status: "inactive", version: "5.3.5" },
    ],
    theme: "storefront",
    created_at: "2026-07-10T09:00:00Z",
    source_site_name: "Pixel Bakery",
    db_bytes: 3_120_400,
    code_bytes: 168_442_000,
  },
  {
    id: "agency-base",
    name: "Agency Base",
    description: "ACF + our block library and brand theme — the starting point for a client build.",
    wp_version: "6.6",
    php_version: "8.2",
    plugins: [
      { name: "advanced-custom-fields", status: "active", version: "6.3.12" },
      { name: "wordfence", status: "active", version: "8.0.3" },
    ],
    theme: "twentytwentyfive",
    created_at: "2026-06-28T14:30:00Z",
    source_site_name: "Acme Corporate",
    db_bytes: 1_902_880,
    code_bytes: 52_004_120,
  },
];

export const syncHistory: Record<string, SyncRecord[]> = {
  "site-pixel-bakery": [
    {
      id: "sync-3",
      site_id: "site-pixel-bakery",
      connection_id: "conn-prod",
      direction: "pull",
      kind: "db",
      status: "success",
      message: "Imported DB from pixelbakery.example (18.4 MB) and rewrote URLs.",
      created_at: "2026-07-18T20:41:00Z",
    },
    {
      id: "sync-2",
      site_id: "site-pixel-bakery",
      connection_id: "conn-prod",
      direction: "push",
      kind: "db",
      status: "success",
      message: "Pushed database to pixel-bakery (Production).",
      created_at: "2026-07-15T14:03:00Z",
    },
    {
      id: "sync-1",
      site_id: "site-pixel-bakery",
      connection_id: "conn-prod",
      direction: "push",
      kind: "code",
      status: "success",
      message: "Pushed wp-content (214 MB) to pixel-bakery (Production).",
      created_at: "2026-07-15T13:58:00Z",
    },
  ],
};
