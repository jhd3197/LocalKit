// Mock of @tauri-apps/api/core `invoke` for the mock build (`vite --mode mock`).
// Returns the fictional data in ./data so the whole UI renders populated with
// no Docker and no Rust backend. State is mutated in memory, so start / stop /
// delete / create behave naturally while previewing.
import * as data from "./data";
import { emit } from "./event";
import type {
  PortConflict,
  RouterStatus,
  ServerKitInfo,
  Site,
  SiteEvent,
  Snapshot,
  SnapshotKind,
} from "../lib/types";

type Args = Record<string, unknown>;

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

/** Plan 16 pre-flight, mocked: which router ports the fake LocalWP holds. */
function mockProbe(http: number, https: number): PortConflict[] {
  return [http, https]
    .filter((p) => p in data.heldPorts)
    .map((port) => ({ port, process: data.heldPorts[port] }));
}

/**
 * Mirror of `router::conflict_message` + the short-circuited status.
 * `keepEnabled` distinguishes the two real cases: a failed *enable* leaves the
 * flag off (the backend never reaches `set_flag`), while a conflict found by a
 * later status poll happens with domains already enabled.
 */
function mockConflict(conflicts: PortConflict[], keepEnabled = false): RouterStatus {
  const held = conflicts.map((c) => `port ${c.port} is held by ${c.process}`).join(", ");
  const onDefaults = data.routerStatus.http_port === 80 && data.routerStatus.https_port === 443;
  data.routerStatus.running = false;
  if (!keepEnabled) data.routerStatus.enabled = false;
  data.routerStatus.conflicts = conflicts;
  data.routerStatus.error =
    `Local domains could not start: ${held}. ` +
    (onDefaults
      ? "Quit the other program (LocalWP's router, IIS, Skype, or another web server), " +
        "or switch LocalKit to fallback ports (8080/8443) in Settings → Domains."
      : "Quit whatever is holding those ports, or pick different router ports in Settings → Domains.");
  return { ...data.routerStatus };
}

// Fake interactive shells for the Terminal page (terminal_open/write/close).
const mockShells = new Map<string, { slug: string; buffer: string }>();
/** Site ids whose in-flight mock transfer has been asked to stop (plan 19). */
const mockCancels = new Set<string>();
const prompt = (slug: string) =>
  `\x1b[35mroot\x1b[0m@\x1b[34m${slug}\x1b[0m:\x1b[36m/var/www/html\x1b[0m# `;

/** Add a snapshot to the fake store, newest first (mirrors the real listing). */
function pushSnapshot(siteId: string, kind: SnapshotKind, note: string): Snapshot {
  const site = data.sites.find((s) => s.id === siteId);
  const now = new Date();
  const snap: Snapshot = {
    // Same shape as the Rust id: a sortable timestamp.
    id: now.toISOString().replace(/[-:T]/g, "").slice(0, 14) + `-${now.getMilliseconds()}`,
    site_id: siteId,
    site_name: site?.name ?? siteId,
    site_slug: site?.slug ?? siteId,
    created_at: now.toISOString(),
    kind,
    note,
    db_bytes: 2_000_000 + Math.floor(Math.random() * 3_000_000),
    code_bytes: 40_000_000 + Math.floor(Math.random() * 200_000_000),
    wp_version: site?.wp_version ?? "6.7",
  };
  (data.snapshots[siteId] ??= []).unshift(snap);
  return snap;
}

function slugify(name: string): string {
  return name
    .toLowerCase()
    .trim()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
}

// Mock-only escape hatch: lets the headless verification scripts put the fake
// backend into states the UI alone can't reach (e.g. "the router died while
// domains were enabled"). Never present in the real app.
if (typeof window !== "undefined") {
  (window as unknown as { __LOCALKIT_MOCK__?: typeof data }).__LOCALKIT_MOCK__ = data;
}

export async function invoke<T = unknown>(cmd: string, args: Args = {}): Promise<T> {
  // Small latency so loading states behave like the real backend — but
  // terminal keystrokes must echo immediately.
  if (!cmd.startsWith("terminal_")) await sleep(120);
  return (await dispatch(cmd, args)) as T;
}

async function dispatch(cmd: string, a: Args): Promise<unknown> {
  switch (cmd) {
    case "check_docker":
      return { available: true, version: "27.5.1", error: null };

    case "app_info":
      return data.appInfo;

    case "list_sites":
      return data.sites.map(({ db_password: _pw, ...s }) => s);

    case "get_site": {
      const site = data.sites.find((s) => s.id === a.id);
      if (!site) throw `site not found: ${a.id}`;
      return data.siteDetail(site);
    }

    case "create_site": {
      const name = String(a.name ?? "").trim();
      if (!name) throw "Site name is required.";
      const slug = slugify(name);
      const port = Math.max(...data.sites.map((s) => s.port), 8080) + 1;
      const id = `site-${slug}`;
      // Fake the staged progress the Rust backend emits while creating.
      const stages: Array<[string, string]> = [
        ["files", "Writing Docker Compose files…"],
        ["pulling", "Downloading WordPress images (first run can take a few minutes)…"],
        ["containers", "Starting containers…"],
        ["waiting", "Waiting for WordPress to respond…"],
        ["install", "Installing WordPress…"],
        ["done", `Site "${name}" is ready at http://localhost:${port}`],
      ];
      void (async () => {
        for (const [stage, message] of stages) {
          emit("site-event", { id, stage, message } satisfies SiteEvent);
          await sleep(900);
        }
      })();
      const site: Site = {
        id,
        name,
        slug,
        path: `${data.appInfo.sites_dir}\\${slug}`,
        port,
        wp_version: String(a.wpVersion),
        php_version: String(a.phpVersion),
        status: "creating",
        admin_user: "admin",
        admin_pass: "generated-demo-pass",
        created_at: new Date().toISOString(),
        connection_id: null,
        remote_site_id: null,
      };
      data.sites.push({ ...site, live_status: "creating", db_password: "m4ri4-n3w-0000" });
      // Flip to running once the fake install finishes.
      void (async () => {
        await sleep(900 * stages.length);
        const s = data.sites.find((x) => x.id === id);
        if (s) {
          s.status = "running";
          s.live_status = "running";
        }
      })();
      return site;
    }

    case "clone_site": {
      const source = data.sites.find((s) => s.id === a.id);
      if (!source) throw `site not found: ${a.id}`;
      const name = String(a.newName ?? "").trim();
      if (!name) throw "Site name is required";
      const slug = slugify(name);
      const port = Math.max(...data.sites.map((s) => s.port), 8080) + 1;
      const id = `site-${slug}`;
      // Same stages the Rust clone emits; the `snapshot` one carries the
      // *source* id (that's the site being read), the rest the new clone.
      const stages: Array<[string, string, string]> = [
        [a.id as string, "snapshot", "Exporting database…"],
        [a.id as string, "snapshot", "Archiving wp-content…"],
        [id, "files", "Writing project files…"],
        [id, "containers", "Starting Docker containers…"],
        [id, "waiting", "Waiting for WordPress to come online…"],
        [id, "import", `Copying ${source.name}'s content…`],
        [id, "import", "Rewriting URLs to the clone…"],
        [id, "done", `${name} cloned from ${source.name} — now running at http://localhost:${port}`],
      ];
      void (async () => {
        for (const [eid, stage, message] of stages) {
          emit("site-event", { id: eid, stage, message } satisfies SiteEvent);
          await sleep(700);
        }
        const s = data.sites.find((x) => x.id === id);
        if (s) s.status = s.live_status = "running";
      })();

      const clone: Site = {
        id,
        name,
        slug,
        path: `${data.appInfo.sites_dir}\\${slug}`,
        port,
        wp_version: source.wp_version,
        php_version: source.php_version,
        status: "creating",
        // The cloned database carries the source's login, so its admin
        // credentials work on the copy; ports and DB secrets are fresh.
        admin_user: source.admin_user,
        admin_pass: source.admin_pass,
        created_at: new Date().toISOString(),
        connection_id: null,
        remote_site_id: null,
      };
      data.sites.push({ ...clone, live_status: "creating", db_password: "m4ri4-cl0ne-0001" });
      return clone;
    }

    case "start_site": {
      const site = data.sites.find((s) => s.id === a.id);
      if (!site) throw `site not found: ${a.id}`;
      site.status = site.live_status = "running";
      return site;
    }

    case "stop_site": {
      const site = data.sites.find((s) => s.id === a.id);
      if (!site) throw `site not found: ${a.id}`;
      site.status = site.live_status = "stopped";
      return site;
    }

    case "delete_site": {
      const siteId = String(a.id);
      const i = data.sites.findIndex((s) => s.id === siteId);
      if (i >= 0) {
        // Mirrors site::delete — a pre_delete snapshot first, and the
        // snapshots outlive the site unless the caller opted out.
        if (a.deleteSnapshots) {
          delete data.snapshots[siteId];
        } else {
          pushSnapshot(siteId, "pre_delete", `before deleting ${data.sites[i].name}`);
        }
        data.sites.splice(i, 1);
      }
      return null;
    }

    case "list_snapshots":
      return data.snapshots[String(a.siteId)] ?? [];

    case "create_snapshot": {
      const siteId = String(a.siteId);
      const site = data.sites.find((s) => s.id === siteId);
      if (!site) throw `site not found: ${siteId}`;
      const snap = pushSnapshot(siteId, "manual", String(a.note ?? ""));
      emit("site-event", {
        id: siteId,
        stage: "done",
        message: `Snapshot of ${site.name} taken`,
      } satisfies SiteEvent);
      return snap;
    }

    case "restore_snapshot": {
      const siteId = String(a.siteId);
      const site = data.sites.find((s) => s.id === siteId);
      if (!site) throw `site not found: ${siteId}`;
      const snap = (data.snapshots[siteId] ?? []).find((s) => s.id === a.snapshotId);
      if (!snap) throw `snapshot \`${a.snapshotId}\` not found`;
      // Restoring is destructive, so the real backend snapshots first.
      pushSnapshot(siteId, "pre_restore", `before restoring ${snap.created_at}`);
      const started = site.live_status !== "running";
      site.status = site.live_status = "running";
      void (async () => {
        for (const message of [
          "Taking a pre-restore snapshot…",
          "Importing database…",
          "Restoring wp-content…",
        ]) {
          emit("site-event", { id: siteId, stage: "restore", message } satisfies SiteEvent);
          await sleep(700);
        }
        emit("site-event", {
          id: siteId,
          stage: "done",
          message: started
            ? `${site.name} restored to the snapshot from ${snap.created_at} (the site was stopped, so it was started)`
            : `${site.name} restored to the snapshot from ${snap.created_at}`,
        } satisfies SiteEvent);
      })();
      return null;
    }

    case "delete_snapshot": {
      const list = data.snapshots[String(a.siteId)] ?? [];
      const i = list.findIndex((s) => s.id === a.snapshotId);
      if (i < 0) throw `snapshot \`${a.snapshotId}\` not found`;
      list.splice(i, 1);
      return null;
    }

    case "site_logs":
      return data.siteLogs[String(a.id)] ?? "No logs yet.";

    case "wp_cli_info": {
      const site = data.sites.find((s) => s.id === a.id);
      if (!site) throw `site not found: ${a.id}`;
      if (site.live_status !== "running") throw "site is not running";
      return data.wpInfo[site.id] ?? { core_version: site.wp_version, plugins: [] };
    }

    case "login_site": {
      const site = data.sites.find((s) => s.id === a.id);
      if (!site) throw `site not found: ${a.id}`;
      if (site.live_status !== "running") {
        throw `"${site.name}" is not running — start the site first.`;
      }
      return `http://localhost:${site.port}/wp-login.php?localkit-login=mock-token&uid=${a.userId ?? 1}`;
    }

    case "site_wp_users": {
      const site = data.sites.find((s) => s.id === a.id);
      if (!site) throw `site not found: ${a.id}`;
      if (site.live_status !== "running") throw "site is not running";
      return [
        { id: 1, login: site.admin_user, name: "Site Admin", roles: "administrator" },
        { id: 2, login: "editor", name: "Demo Editor", roles: "editor" },
      ];
    }

    case "save_serverkit_connection": {
      const conn = {
        id: `conn-${slugify(String(a.label))}`,
        label: String(a.label),
        url: String(a.url),
        api_key: String(a.apiKey),
        created_at: new Date().toISOString(),
      };
      data.connections.push(conn);
      return conn;
    }

    case "list_serverkit_connections":
      return data.connections;

    case "delete_serverkit_connection": {
      const i = data.connections.findIndex((c) => c.id === a.id);
      if (i >= 0) data.connections.splice(i, 1);
      return null;
    }

    case "test_serverkit_connection": {
      const info: ServerKitInfo = {
        status: "ok",
        service: "serverkit",
        canonical_domain: new URL(String(a.url)).hostname,
        canonical_origin: String(a.url),
        staging: false,
        api_key_valid: true,
        localkit_extension: true,
        features: ["sites", "push-code", "push-db", "pull-db", "pull-code"],
      };
      return info;
    }

    case "list_remote_wp_sites":
      return data.remoteSites[String(a.id)] ?? [];

    case "create_remote_site": {
      const list = data.remoteSites[String(a.connectionId)] ?? [];
      const site = {
        id: Math.max(...list.map((s) => s.id), 0) + 1,
        name: slugify(String(a.name)),
        url: `https://${slugify(String(a.name))}.example`,
        status: "running",
        wp_version: "6.7",
        php_version: "8.3",
        multisite: false,
        environment_count: 1,
      };
      list.push(site);
      data.remoteSites[String(a.connectionId)] = list;
      return site;
    }

    case "import_remote_site": {
      const connectionId = String(a.connectionId);
      const remoteId = Number(a.remoteSiteId);
      const remote = (data.remoteSites[connectionId] ?? []).find((s) => s.id === remoteId);
      if (!remote) throw `Remote site #${remoteId} was not found.`;
      if (remote.multisite) {
        throw `"${remote.name}" is a WordPress multisite install, which LocalKit cannot import.`;
      }
      if (data.sites.some((s) => s.connection_id === connectionId && s.remote_site_id === remoteId)) {
        throw `"${remote.name}" was already imported. Pull its database into that site instead.`;
      }

      const name = String(a.name ?? "").trim() || remote.name;
      const slug = slugify(name);
      const port = Math.max(...data.sites.map((s) => s.port), 8080) + 1;
      const id = `site-${slug}`;
      const conn = data.connections.find((c) => c.id === connectionId);
      // Same stages the Rust import emits, so the progress toast reads the same.
      const stages: Array<[string, string]> = [
        ["files", "Writing project files…"],
        ["pulling", "Downloading WordPress images (first run can take a few minutes)…"],
        ["code", "Downloading remote wp-content…"],
        ["code", "Extracting wp-content (48.2 MB)…"],
        ["containers", "Starting Docker containers…"],
        ["waiting", "Waiting for WordPress to come online…"],
        ["install", "Downloading remote database…"],
        ["install", "Importing remote database…"],
        ["install", "Rewriting URLs remote -> local…"],
        ["done", `${name} imported from ${conn?.label ?? "server"} — now running at http://localhost:${port}`],
      ];
      void (async () => {
        for (const [stage, message] of stages) {
          emit("site-event", { id, stage, message } satisfies SiteEvent);
          await sleep(700);
        }
        const s = data.sites.find((x) => x.id === id);
        if (s) s.status = s.live_status = "running";
      })();

      const site: Site = {
        id,
        name,
        slug,
        path: `${data.appInfo.sites_dir}\\${slug}`,
        port,
        wp_version: data.appInfo.wp_versions.includes(remote.wp_version ?? "")
          ? String(remote.wp_version)
          : data.appInfo.wp_versions[0],
        php_version: data.appInfo.php_versions.includes(remote.php_version ?? "")
          ? String(remote.php_version)
          : data.appInfo.php_versions[0],
        status: "creating",
        // The imported database keeps the remote's accounts, and no password
        // of ours — mirrors what the backend records.
        admin_user: "admin",
        admin_pass: "",
        created_at: new Date().toISOString(),
        connection_id: connectionId,
        remote_site_id: remoteId,
      };
      data.sites.push({ ...site, live_status: "creating", db_password: "m4ri4-imp0rt-0001" });
      (data.syncHistory[id] ??= []).unshift({
        id: `sync-${Date.now()}`,
        site_id: id,
        connection_id: connectionId,
        direction: "pull",
        kind: "import",
        status: "success",
        message: `${name} imported from ${conn?.label ?? "server"} via mock.`,
        created_at: new Date().toISOString(),
      });
      return site;
    }

    case "push_site_code":
    case "push_site_db":
    case "pull_site_db": {
      const kind = cmd === "push_site_code" ? "code" : "db";
      const direction = cmd === "pull_site_db" ? "pull" : "push";
      const siteId = String(a.siteId);
      const site = data.sites.find((s) => s.id === siteId);
      const id = `sync-${Date.now()}`;
      // Plan 17: DB syncs overwrite a database, so they snapshot first.
      if (kind === "db") {
        const conn = data.connections.find((c) => c.id === a.connectionId);
        pushSnapshot(
          siteId,
          direction === "push" ? "pre_push" : "pre_pull",
          `${conn?.label ?? "server"} (#${a.remoteSiteId} on ${conn?.url ?? "?"})`
        );
      }
      const verb = direction === "push" ? "Pushing" : "Pulling";
      const past = direction === "push" ? "Pushed" : "Pulled";
      const record = {
        id,
        site_id: siteId,
        connection_id: String(a.connectionId),
        direction,
        kind,
        status: "success",
        message: `${past} ${kind} via mock.`,
        created_at: new Date().toISOString(),
      };
      (data.syncHistory[siteId] ??= []).unshift(record);

      // Plan 19: a chunked transfer, so the mock UI shows the same byte
      // readout and Cancel button the real backend drives.
      void (async () => {
        mockCancels.delete(siteId);
        const total = 312 * 1024 * 1024;
        const chunk = 8 * 1024 * 1024;
        emit("site-event", {
          id: siteId,
          stage: direction,
          message: kind === "code" ? "Bundling wp-content…" : "Exporting local database…",
        } satisfies SiteEvent);
        await sleep(500);

        for (let done = 0; done <= total; done += chunk) {
          if (mockCancels.delete(siteId)) {
            record.status = "cancelled";
            record.message = `${direction} ${kind} cancelled`;
            emit("site-event", {
              id: siteId,
              stage: "cancelled",
              message: `${direction} ${kind} cancelled`,
            } satisfies SiteEvent);
            return;
          }
          emit("site-event", {
            id: siteId,
            stage: direction,
            message: `${verb} ${kind === "code" ? "wp-content" : "database"}`,
            bytes_done: Math.min(done, total),
            bytes_total: total,
          } satisfies SiteEvent);
          await sleep(80);
        }
        emit("site-event", {
          id: siteId,
          stage: "done",
          message: `${past} ${kind} for ${site?.name}.`,
        } satisfies SiteEvent);
      })();
      return null;
    }

    case "cancel_sync": {
      const siteId = String(a.siteId);
      mockCancels.add(siteId);
      return true;
    }

    case "list_sync_history":
      return data.syncHistory[String(a.siteId)] ?? [];

    case "router_status": {
      // Mirrors `router::status`: re-diagnose whenever domains are enabled but
      // the router is down, so a conflict that appeared *after* enabling (the
      // real hazard — LocalWP launched later, or won the boot race) keeps
      // reporting its named cause.
      const s = data.routerStatus;
      if (s.enabled && !s.running) {
        const conflicts = mockProbe(s.http_port, s.https_port);
        if (conflicts.length > 0) return mockConflict(conflicts, true);
      }
      return { ...s };
    }

    case "set_domains_enabled": {
      const enabled = Boolean(a.enabled);
      if (enabled) {
        const conflicts = mockProbe(data.routerStatus.http_port, data.routerStatus.https_port);
        if (conflicts.length > 0) return mockConflict(conflicts);
      }
      data.routerStatus.enabled = enabled;
      data.routerStatus.running = enabled;
      data.routerStatus.error = null;
      data.routerStatus.conflicts = [];
      return { ...data.routerStatus };
    }

    case "set_router_ports": {
      const http = Number(a.http);
      const https = Number(a.https);
      if (!http || !https) throw "Router ports must be between 1 and 65535.";
      if (http === https) throw "The HTTP and HTTPS router ports must be different.";
      if (data.routerStatus.enabled) {
        const conflicts = mockProbe(http, https);
        if (conflicts.length > 0) return mockConflict(conflicts);
        data.routerStatus.running = true;
      }
      data.routerStatus.http_port = http;
      data.routerStatus.https_port = https;
      data.routerStatus.error = null;
      data.routerStatus.conflicts = [];
      return { ...data.routerStatus };
    }

    case "trust_router_ca":
      data.routerStatus.ca_trusted = true;
      return { ...data.routerStatus };

    case "get_app_setting":
      return (data.appSettings[String(a.key)] as string | undefined) ?? null;

    case "set_app_setting":
      data.appSettings[String(a.key)] = String(a.value);
      return null;

    case "delete_app_setting":
      delete data.appSettings[String(a.key)];
      return null;

    case "settings_get_all":
      return { ...data.appSettings };

    case "terminal_open": {
      const site = data.sites.find((s) => s.id === a.siteId);
      if (!site) throw `site not found: ${a.siteId}`;
      if (site.live_status !== "running") {
        throw `"${site.name}" is not running — start the site first.`;
      }
      const id = `mock-term-${site.id}-${Date.now()}`;
      mockShells.set(id, { slug: site.slug, buffer: "" });
      setTimeout(() => {
        emit("terminal://data", {
          terminalId: id,
          data: `\x1b[90m# mock shell inside ${site.slug}'s wordpress container\x1b[0m\r\n`,
        });
        emit("terminal://data", { terminalId: id, data: prompt(site.slug) });
      }, 250);
      return id;
    }

    case "terminal_write": {
      const shell = mockShells.get(String(a.terminalId));
      if (!shell) return null;
      const input = String(a.data);
      for (const ch of input) {
        if (ch === "\r") {
          const line = shell.buffer;
          shell.buffer = "";
          emit("terminal://data", { terminalId: a.terminalId, data: "\r\n" });
          if (line.trim()) {
            emit("terminal://data", {
              terminalId: a.terminalId,
              data: `\x1b[90mbash: ${line.trim().split(/\s+/)[0]}: command not found (mock)\x1b[0m\r\n`,
            });
          }
          emit("terminal://data", { terminalId: a.terminalId, data: prompt(shell.slug) });
        } else if (ch === "" || ch === "\b") {
          if (shell.buffer.length > 0) {
            shell.buffer = shell.buffer.slice(0, -1);
            emit("terminal://data", { terminalId: a.terminalId, data: "\b \b" });
          }
        } else if (ch >= " ") {
          shell.buffer += ch;
          emit("terminal://data", { terminalId: a.terminalId, data: ch });
        }
      }
      return null;
    }

    case "terminal_resize":
      return null;

    case "terminal_close": {
      mockShells.delete(String(a.terminalId));
      return null;
    }

    default:
      console.warn(`[mock] unhandled command: ${cmd}`);
      return null;
  }
}
