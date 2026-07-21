import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AppInfo,
  Blueprint,
  DebugStatus,
  DockerProjectInspection,
  DockerStatus,
  RemoteWpSite,
  RouterStatus,
  SearchReplaceResult,
  ServerKitConnection,
  ServerKitInfo,
  Site,
  SiteDetail,
  SiteEvent,
  SiteWithStatus,
  Snapshot,
  SyncRecord,
  TerminalDataEvent,
  TerminalExitEvent,
  WpInfo,
  WpUser,
} from "./types";

/** Typed wrappers around the Tauri commands exposed by the Rust backend. */
export const ipc = {
  checkDocker: (force = false) => invoke<DockerStatus>("check_docker", { force }),
  appInfo: () => invoke<AppInfo>("app_info"),
  listSites: () => invoke<SiteWithStatus[]>("list_sites"),
  getSite: (id: string) => invoke<SiteDetail>("get_site", { id }),
  createSite: (name: string, wpVersion: string, phpVersion: string) =>
    invoke<Site>("create_site", { name, wpVersion, phpVersion }),
  /** Inspect a folder as a candidate Docker project (plan 22). */
  inspectDockerProject: (path: string) =>
    invoke<DockerProjectInspection>("inspect_docker_project", { path }),
  /** Import a Docker project as a new local site (plan 22). */
  importDockerProject: (
    name: string,
    path: string,
    service: string,
    appPort: number,
    includeAll = false
  ) => invoke<Site>("import_docker_project", { name, path, service, appPort, includeAll }),
  cloneSite: (id: string, newName: string) =>
    invoke<Site>("clone_site", { id, newName }),
  startSite: (id: string) => invoke<Site>("start_site", { id }),
  stopSite: (id: string) => invoke<Site>("stop_site", { id }),
  /** Finish a half-created site (plan 23). */
  resumeSite: (id: string) => invoke<Site>("resume_site", { id }),
  deleteSite: (id: string, deleteSnapshots = false) =>
    invoke<void>("delete_site", { id, deleteSnapshots }),
  siteLogs: (id: string, tail = 200) => invoke<string>("site_logs", { id, tail }),
  wpCliInfo: (id: string) => invoke<WpInfo>("wp_cli_info", { id }),
  /** Serialization-safe search-replace (plan 24); dryRun counts without writing. */
  siteSearchReplace: (id: string, from: string, to: string, dryRun: boolean) =>
    invoke<SearchReplaceResult>("site_search_replace", { id, from, to, dryRun }),
  /** WP_DEBUG state + debug-log size (plan 24). */
  siteDebugStatus: (id: string) => invoke<DebugStatus>("site_debug_status", { id }),
  /** Toggle WP_DEBUG + WP_DEBUG_LOG (log to file, never screen) (plan 24). */
  setSiteDebug: (id: string, enabled: boolean) =>
    invoke<DebugStatus>("set_site_debug", { id, enabled }),
  /** Tail of wp-content/debug.log (plan 24). */
  readSiteDebugLog: (id: string) => invoke<string>("read_site_debug_log", { id }),
  /** Truncate the debug log (plan 24). */
  clearSiteDebugLog: (id: string) => invoke<void>("clear_site_debug_log", { id }),
  loginSite: (id: string, userId?: number) => invoke<string>("login_site", { id, userId }),
  siteWpUsers: (id: string) => invoke<WpUser[]>("site_wp_users", { id }),
  listSnapshots: (siteId: string) => invoke<Snapshot[]>("list_snapshots", { siteId }),
  createSnapshot: (siteId: string, note?: string) =>
    invoke<Snapshot>("create_snapshot", { siteId, note }),
  restoreSnapshot: (siteId: string, snapshotId: string) =>
    invoke<void>("restore_snapshot", { siteId, snapshotId }),
  deleteSnapshot: (siteId: string, snapshotId: string) =>
    invoke<void>("delete_snapshot", { siteId, snapshotId }),
  listBlueprints: () => invoke<Blueprint[]>("list_blueprints"),
  saveBlueprint: (siteId: string, name: string, description?: string) =>
    invoke<Blueprint>("save_blueprint", { siteId, name, description }),
  deleteBlueprint: (id: string) => invoke<void>("delete_blueprint", { id }),
  createSiteFromBlueprint: (blueprintId: string, name?: string) =>
    invoke<Site>("create_site_from_blueprint", { blueprintId, name }),
  saveServerkitConnection: (label: string, url: string, apiKey: string) =>
    invoke<ServerKitConnection>("save_serverkit_connection", { label, url, apiKey }),
  listServerkitConnections: () => invoke<ServerKitConnection[]>("list_serverkit_connections"),
  deleteServerkitConnection: (id: string) => invoke<void>("delete_serverkit_connection", { id }),
  testServerkitConnection: (url: string, apiKey: string) =>
    invoke<ServerKitInfo>("test_serverkit_connection", { url, apiKey }),
  listRemoteWpSites: (id: string) => invoke<RemoteWpSite[]>("list_remote_wp_sites", { id }),
  createRemoteSite: (connectionId: string, name: string) =>
    invoke<unknown>("create_remote_site", { connectionId, name }),
  pushSiteCode: (connectionId: string, siteId: string, remoteSiteId: number) =>
    invoke<void>("push_site_code", { connectionId, siteId, remoteSiteId }),
  pushSiteDb: (connectionId: string, siteId: string, remoteSiteId: number) =>
    invoke<void>("push_site_db", { connectionId, siteId, remoteSiteId }),
  pullSiteDb: (connectionId: string, siteId: string, remoteSiteId: number, remoteUrl: string | null) =>
    invoke<void>("pull_site_db", { connectionId, siteId, remoteSiteId, remoteUrl }),
  importRemoteSite: (connectionId: string, remoteSiteId: number, name?: string) =>
    invoke<Site>("import_remote_site", { connectionId, remoteSiteId, name }),
  listSyncHistory: (siteId: string) => invoke<SyncRecord[]>("list_sync_history", { siteId }),
  /** Stop the in-flight chunked sync for a site; resolves to whether there was one. */
  cancelSync: (siteId: string) => invoke<boolean>("cancel_sync", { siteId }),
  routerStatus: () => invoke<RouterStatus>("router_status"),
  setDomainsEnabled: (enabled: boolean) =>
    invoke<RouterStatus>("set_domains_enabled", { enabled }),
  setRouterPorts: (http: number, https: number) =>
    invoke<RouterStatus>("set_router_ports", { http, https }),
  trustRouterCa: () => invoke<RouterStatus>("trust_router_ca"),
  getAppSetting: (key: string) => invoke<string | null>("get_app_setting", { key }),
  setAppSetting: (key: string, value: string) =>
    invoke<void>("set_app_setting", { key, value }),
  deleteAppSetting: (key: string) => invoke<void>("delete_app_setting", { key }),
  settingsGetAll: () => invoke<Record<string, string>>("settings_get_all"),
  terminalOpen: (siteId: string, cols: number, rows: number) =>
    invoke<string>("terminal_open", { siteId, cols, rows }),
  terminalWrite: (terminalId: string, data: string) =>
    invoke<void>("terminal_write", { terminalId, data }),
  terminalResize: (terminalId: string, cols: number, rows: number) =>
    invoke<void>("terminal_resize", { terminalId, cols, rows }),
  terminalClose: (terminalId: string) => invoke<void>("terminal_close", { terminalId }),
};

/** Subscribe to progress events emitted during long operations (site create). */
export function onSiteEvent(cb: (event: SiteEvent) => void): Promise<UnlistenFn> {
  return listen<SiteEvent>("site-event", (e) => cb(e.payload));
}

/**
 * Fired after the reconciler settles a site's status against Docker ground
 * truth (plan 23) — the frontend re-fetches so a site stopped/started outside
 * the app corrects itself without a manual refresh.
 */
export function onSitesChanged(cb: () => void): Promise<UnlistenFn> {
  return listen("sites-changed", () => cb());
}

/** Terminal output stream for one PTY session (filter by terminalId). */
export function onTerminalData(cb: (event: TerminalDataEvent) => void): Promise<UnlistenFn> {
  return listen<TerminalDataEvent>("terminal://data", (e) => cb(e.payload));
}

/** Fired when a terminal's shell exits (site stopped, `exit`, close). */
export function onTerminalExit(cb: (event: TerminalExitEvent) => void): Promise<UnlistenFn> {
  return listen<TerminalExitEvent>("terminal://exit", (e) => cb(e.payload));
}
