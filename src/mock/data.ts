// Fictional data for the mock build (`vite --mode mock`). Every hostname,
// credential and path below is made up — these exist so the UI renders
// populated for screenshots and manual previews, with no Docker or Tauri.
import type {
  AppInfo,
  RemoteWpSite,
  RouterStatus,
  ServerKitConnection,
  SiteDetail,
  SiteWithStatus,
  SyncRecord,
  WpInfo,
} from "../lib/types";

// Keep in sync with WP_VERSIONS / PHP_VERSIONS in src-tauri/src/site.rs.
export const appInfo: AppInfo = {
  data_dir: "C:\\Users\\demo\\AppData\\Roaming\\localkit",
  sites_dir: "C:\\Users\\demo\\AppData\\Roaming\\localkit\\sites",
  wp_versions: ["6.7", "6.6", "6.5"],
  php_versions: ["8.3", "8.2", "8.1"],
};

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
};

// M6 local domains: fictional router state. Enabled + running so dashboard /
// detail shots render `*.test` URLs; CA untrusted until trust_router_ca.
export const routerStatus: RouterStatus = {
  enabled: true,
  running: true,
  ca_trusted: false,
  error: null,
  conflicts: [],
};

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
      environment_count: 2,
    },
    {
      id: 27,
      name: "pixel-bakery",
      url: "https://pixelbakery.example",
      status: "running",
      wp_version: "6.7",
      environment_count: 3,
    },
    {
      id: 31,
      name: "landing-tests",
      url: null,
      status: "stopped",
      wp_version: "6.6",
      environment_count: 1,
    },
  ],
};

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
