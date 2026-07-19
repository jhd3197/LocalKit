import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AppInfo,
  DockerStatus,
  RemoteWpSite,
  RouterStatus,
  ServerKitConnection,
  ServerKitInfo,
  Site,
  SiteDetail,
  SiteEvent,
  SiteWithStatus,
  SyncRecord,
  WpInfo,
} from "./types";

/** Typed wrappers around the Tauri commands exposed by the Rust backend. */
export const ipc = {
  checkDocker: () => invoke<DockerStatus>("check_docker"),
  appInfo: () => invoke<AppInfo>("app_info"),
  listSites: () => invoke<SiteWithStatus[]>("list_sites"),
  getSite: (id: string) => invoke<SiteDetail>("get_site", { id }),
  createSite: (name: string, wpVersion: string, phpVersion: string) =>
    invoke<Site>("create_site", { name, wpVersion, phpVersion }),
  startSite: (id: string) => invoke<Site>("start_site", { id }),
  stopSite: (id: string) => invoke<Site>("stop_site", { id }),
  deleteSite: (id: string) => invoke<void>("delete_site", { id }),
  siteLogs: (id: string, tail = 200) => invoke<string>("site_logs", { id, tail }),
  wpCliInfo: (id: string) => invoke<WpInfo>("wp_cli_info", { id }),
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
  listSyncHistory: (siteId: string) => invoke<SyncRecord[]>("list_sync_history", { siteId }),
  routerStatus: () => invoke<RouterStatus>("router_status"),
  setDomainsEnabled: (enabled: boolean) =>
    invoke<RouterStatus>("set_domains_enabled", { enabled }),
  trustRouterCa: () => invoke<RouterStatus>("trust_router_ca"),
  getAppSetting: (key: string) => invoke<string | null>("get_app_setting", { key }),
  setAppSetting: (key: string, value: string) =>
    invoke<void>("set_app_setting", { key, value }),
};

/** Subscribe to progress events emitted during long operations (site create). */
export function onSiteEvent(cb: (event: SiteEvent) => void): Promise<UnlistenFn> {
  return listen<SiteEvent>("site-event", (e) => cb(e.payload));
}
