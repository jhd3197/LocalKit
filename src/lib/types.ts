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

export interface AppInfo {
  data_dir: string;
  sites_dir: string;
  wp_versions: string[];
  php_versions: string[];
}

export interface SiteEvent {
  id: string;
  stage: string;
  message: string;
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
}

export interface RemoteWpSite {
  id: number;
  name: string;
  url: string | null;
  status: string;
  wp_version: string | null;
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
