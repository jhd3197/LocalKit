// Mock of @tauri-apps/api/core `invoke` for the mock build (`vite --mode mock`).
// Returns the fictional data in ./data so the whole UI renders populated with
// no Docker and no Rust backend. State is mutated in memory, so start / stop /
// delete / create behave naturally while previewing.
import * as data from "./data";
import { emit } from "./event";
import type { ServerKitInfo, Site, SiteEvent } from "../lib/types";

type Args = Record<string, unknown>;

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

// Fake interactive shells for the Terminal page (terminal_open/write/close).
const mockShells = new Map<string, { slug: string; buffer: string }>();
const prompt = (slug: string) =>
  `\x1b[35mroot\x1b[0m@\x1b[34m${slug}\x1b[0m:\x1b[36m/var/www/html\x1b[0m# `;

function slugify(name: string): string {
  return name
    .toLowerCase()
    .trim()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
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
      const i = data.sites.findIndex((s) => s.id === a.id);
      if (i >= 0) data.sites.splice(i, 1);
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
        environment_count: 1,
      };
      list.push(site);
      data.remoteSites[String(a.connectionId)] = list;
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
      void (async () => {
        emit("site-event", {
          id: siteId,
          stage: "push",
          message: `${direction === "push" ? "Pushing" : "Pulling"} ${kind} for ${site?.name}…`,
        } satisfies SiteEvent);
        await sleep(1200);
        emit("site-event", {
          id: siteId,
          stage: "done",
          message: `${direction === "push" ? "Pushed" : "Pulled"} ${kind} for ${site?.name}.`,
        } satisfies SiteEvent);
      })();
      const record = {
        id,
        site_id: siteId,
        connection_id: String(a.connectionId),
        direction,
        kind,
        status: "success",
        message: `${direction === "push" ? "Pushed" : "Pulled"} ${kind} via mock.`,
        created_at: new Date().toISOString(),
      };
      (data.syncHistory[siteId] ??= []).unshift(record);
      return null;
    }

    case "list_sync_history":
      return data.syncHistory[String(a.siteId)] ?? [];

    case "router_status":
      return { ...data.routerStatus };

    case "set_domains_enabled": {
      const enabled = Boolean(a.enabled);
      data.routerStatus.enabled = enabled;
      data.routerStatus.running = enabled;
      data.routerStatus.error = null;
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
