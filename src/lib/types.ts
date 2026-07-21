/** Site kinds (plan 22). Mirrors `site::KIND_*` in the backend. */
export const KIND_WORDPRESS = "wordpress";
export const KIND_DOCKER = "docker";

/**
 * What a site's kind (+ config) supports (plan 22). Every capability the UI
 * gates on lives here — WordPress claims all of them; a docker app claims
 * domains/terminal/logs/snapshots/code_sync.
 */
export interface Capabilities {
  domains: boolean;
  terminal: boolean;
  logs: boolean;
  snapshots: boolean;
  db_gui: boolean;
  db_sync: boolean;
  code_sync: boolean;
  one_click_login: boolean;
  wp_tools: boolean;
  search_replace: boolean;
}

/** Per-kind settings (plan 22). Optional fields are omitted by the backend. */
export interface SiteConfig {
  service: string;
  sync_path: string;
  /** Host port the app is reachable at; absent = the site port (WordPress). */
  app_port?: number;
  db_engine?: string | null;
  db_service?: string | null;
}

export interface Site {
  id: string;
  name: string;
  slug: string;
  path: string;
  port: number;
  wp_version: string;
  php_version: string;
  status: string;
  admin_user: string;
  admin_pass: string;
  created_at: string;
  /** Plan 18 — set together on sites imported from a ServerKit server. */
  connection_id: string | null;
  remote_site_id: number | null;
  /** Plan 22 — stack kind, its per-kind config, and derived capabilities. */
  kind: string;
  config: SiteConfig;
  capabilities: Capabilities;
}

export interface SiteWithStatus extends Site {
  live_status: string;
}

export interface SiteDetail extends Site {
  live_status: string;
  db_host: string;
  db_port: number;
  db_name: string;
  db_user: string;
  db_password: string;
}

export interface DockerStatus {
  available: boolean;
  version: string | null;
  error: string | null;
}

export interface PluginInfo {
  name: string;
  status: string;
  version: string;
}

export interface WpInfo {
  core_version: string;
  plugins: PluginInfo[];
}

/** A WordPress user, for the WP Admin one-click login picker. */
export interface WpUser {
  id: number;
  login: string;
  name: string;
  roles: string;
}

/** A site kind and the capabilities it advertises (plan 22 `app_info`). */
export interface KindInfo {
  kind: string;
  capabilities: Capabilities;
}

export interface AppInfo {
  data_dir: string;
  sites_dir: string;
  wp_versions: string[];
  php_versions: string[];
  kinds: KindInfo[];
}

/** A service found in a candidate Docker project (plan 22 import dialog). */
export interface DockerService {
  name: string;
  image: string;
  published_ports: number[];
  db_engine: string | null;
}

/** Read-only inspection of a Docker project folder before importing it. */
export interface DockerProjectInspection {
  compose_file: string;
  services: DockerService[];
  suggested_service: string | null;
  suggested_port: number | null;
  db_engine: string | null;
  db_service: string | null;
  copy_bytes: number;
  excluded: string[];
}

export interface SiteEvent {
  id: string;
  stage: string;
  message: string;
  /**
   * Byte counters, present only during a chunked transfer (plan 19). Absent on
   * every other stage, which is what tells the UI to render the plain stage
   * message instead of a byte readout.
   */
  bytes_done?: number;
  bytes_total?: number;
}

export interface ServerKitConnection {
  id: string;
  label: string;
  url: string;
  api_key: string;
  created_at: string;
}

export interface ServerKitInfo {
  status: string;
  service: string;
  canonical_domain: string | null;
  canonical_origin: string | null;
  staging: boolean;
  api_key_valid: boolean;
  localkit_extension: boolean;
  /** Extension capabilities (plan 18). Absent name = unsupported, not unknown. */
  features: string[];
}

/** Capability names reported by the extension's `GET /pair`. */
export const FEATURE_PULL_CODE = "pull-code";

export interface RemoteWpSite {
  id: number;
  name: string;
  url: string | null;
  status: string;
  wp_version: string | null;
  php_version: string | null;
  /** Multisite installs cannot be imported — one compose project, one site. */
  multisite: boolean;
  environment_count: number;
}

export interface SyncRecord {
  id: string;
  site_id: string;
  connection_id: string;
  direction: string;
  kind: string;
  status: string;
  message: string;
  created_at: string;
}

/** Snapshot kinds (plan 17); everything but `manual` is taken automatically. */
export type SnapshotKind =
  | "manual"
  | "pre_push"
  | "pre_pull"
  | "pre_delete"
  | "pre_restore";

/** A point-in-time copy of a site: DB dump + wp-content archive on disk. */
export interface Snapshot {
  id: string;
  site_id: string;
  site_name: string;
  site_slug: string;
  created_at: string;
  kind: SnapshotKind;
  note: string;
  db_bytes: number;
  code_bytes: number;
  wp_version: string;
}

/** A plugin captured in a blueprint (plan 20) — display metadata only. */
export interface BlueprintPlugin {
  name: string;
  status: string;
  version: string;
}

/**
 * A saved site recipe (plan 20): a database + wp-content copy plus the plugin
 * and theme list captured at save time. New sites can be stamped out of one.
 */
export interface Blueprint {
  /** Directory slug — the stable id for create-from / delete / export. */
  id: string;
  name: string;
  description: string;
  wp_version: string;
  php_version: string;
  plugins: BlueprintPlugin[];
  theme: string;
  created_at: string;
  source_site_name: string;
  db_bytes: number;
  code_bytes: number;
}

/** A router port held by another program (plan 16 pre-flight probe). */
export interface PortConflict {
  port: number;
  process: string | null;
}

export interface RouterStatus {
  enabled: boolean;
  running: boolean;
  ca_trusted: boolean;
  error: string | null;
  conflicts: PortConflict[];
  /** Router host ports; 80/443 = clean-URL mode, anything else = fallback. */
  http_port: number;
  https_port: number;
}

export interface TerminalDataEvent {
  terminalId: string;
  data: string;
}

export interface TerminalExitEvent {
  terminalId: string;
  code: number | null;
}
