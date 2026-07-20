import { useCallback, useEffect, useState } from "react";
import { ipc } from "../lib/ipc";
import type { AppInfo, DockerStatus } from "../lib/types";
import { useNav } from "../stores/nav";
import { useTerminalFontSize, useTerminalScrollback } from "../stores/settings";
import { useDialog } from "../hooks/useDialog";
import ServerKitSettings from "../components/ServerKitSettings";
import DomainsSettings from "../components/DomainsSettings";
import KeyboardSettings from "../components/KeyboardSettings";
import { CloseIcon, GlobeIcon, KeyboardIcon, ServerIcon, SlidersIcon, TerminalIcon } from "../components/icons";

type SectionId = "general" | "terminal" | "keyboard" | "domains" | "serverkit";

const SECTIONS: { id: SectionId; label: string; icon: React.ReactNode }[] = [
  { id: "general", label: "General", icon: <SlidersIcon className="h-3.5 w-3.5" /> },
  { id: "terminal", label: "Terminal", icon: <TerminalIcon className="h-3.5 w-3.5" /> },
  { id: "keyboard", label: "Keyboard", icon: <KeyboardIcon className="h-3.5 w-3.5" /> },
  { id: "domains", label: "Local domains", icon: <GlobeIcon className="h-3.5 w-3.5" /> },
  { id: "serverkit", label: "ServerKit", icon: <ServerIcon className="h-3.5 w-3.5" /> },
];

export default function Settings() {
  const setSettingsOpen = useNav((s) => s.setSettingsOpen);
  const [active, setActive] = useState<SectionId>("general");
  const { overlayProps, panelProps } = useDialog(() => setSettingsOpen(false));

  const activeLabel = SECTIONS.find((s) => s.id === active)?.label;

  return (
    <div
      {...overlayProps}
      className="fixed inset-0 z-40 flex items-center justify-center bg-black/60 backdrop-blur-sm"
    >
      <div
        {...panelProps}
        role="dialog"
        aria-modal="true"
        aria-label="Settings"
        className="flex h-[32rem] max-h-[88vh] w-[48rem] max-w-[94vw] overflow-hidden rounded-xl border border-zinc-800 bg-zinc-900 shadow-panel"
      >
        {/* Left section rail — Faro-style */}
        <nav className="flex w-44 shrink-0 flex-col gap-0.5 border-r border-zinc-800 bg-zinc-950/50 p-2">
          <div className="px-2 pb-2 pt-1 text-[15px] font-semibold tracking-tight text-white">
            Settings
          </div>
          {SECTIONS.map((sec) => {
            const on = active === sec.id;
            return (
              <button
                key={sec.id}
                onClick={() => setActive(sec.id)}
                aria-current={on ? "page" : undefined}
                className={`flex items-center gap-2.5 rounded-md px-2.5 py-1.5 text-left text-sm transition-colors ${
                  on
                    ? "bg-violet-500/15 text-violet-400"
                    : "text-zinc-400 hover:bg-zinc-900 hover:text-zinc-100"
                }`}
              >
                <span className={on ? "text-violet-400" : "text-zinc-600"}>{sec.icon}</span>
                <span className="truncate">{sec.label}</span>
              </button>
            );
          })}
        </nav>

        {/* Content pane */}
        <div className="flex min-w-0 flex-1 flex-col">
          <div className="flex items-center border-b border-zinc-800 px-5 py-3">
            <span className="text-sm font-semibold text-white">{activeLabel}</span>
            <div className="flex-1" />
            <button
              onClick={() => setSettingsOpen(false)}
              aria-label="Close settings"
              className="rounded-md p-1.5 text-zinc-500 hover:bg-zinc-800 hover:text-zinc-300"
            >
              <CloseIcon className="h-3.5 w-3.5" />
            </button>
          </div>

          <div className="flex-1 overflow-y-auto px-5 py-4">
            {active === "general" && <GeneralSection />}
            {active === "terminal" && <TerminalSection />}
            {active === "keyboard" && <KeyboardSettings />}
            {active === "domains" && <DomainsSettings />}
            {active === "serverkit" && <ServerKitSettings />}
          </div>
        </div>
      </div>
    </div>
  );
}

function GeneralSection() {
  const [docker, setDocker] = useState<DockerStatus | null>(null);
  const [checking, setChecking] = useState(false);
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [runInBackground, setRunInBackground] = useState(true);

  const checkDocker = useCallback(async () => {
    setChecking(true);
    try {
      setDocker(await ipc.checkDocker());
    } finally {
      setChecking(false);
    }
  }, []);

  useEffect(() => {
    void checkDocker();
    ipc.appInfo().then(setInfo).catch(() => {});
    ipc
      .getAppSetting("run_in_background")
      .then((v) => setRunInBackground(v !== "false"))
      .catch(() => {});
  }, [checkDocker]);

  const toggleRunInBackground = () => {
    const next = !runInBackground;
    setRunInBackground(next);
    void ipc.setAppSetting("run_in_background", String(next)).catch(() => setRunInBackground(!next));
  };

  return (
    <div className="space-y-4">
      {/* Docker */}
      <section className="rounded-xl border border-zinc-800 bg-zinc-950/60 p-4">
        <div className="flex items-center justify-between">
          <h2 className="text-xs font-semibold uppercase tracking-wide text-zinc-500">Docker</h2>
          <button onClick={() => void checkDocker()} className="text-xs text-zinc-500 hover:text-zinc-300">
            {checking ? "Checking…" : "Re-check"}
          </button>
        </div>
        <div className="mt-2.5 flex items-center gap-2 text-sm">
          <span
            className={`h-2.5 w-2.5 rounded-full ${
              docker === null ? "bg-zinc-600" : docker.available ? "bg-emerald-400" : "bg-red-400"
            }`}
          />
          {docker === null ? (
            <span className="text-zinc-500">Checking Docker status…</span>
          ) : docker.available ? (
            <span className="text-zinc-300">
              Docker is running{docker.version ? ` (server ${docker.version})` : ""}
            </span>
          ) : (
            <span className="text-red-300">{docker.error ?? "Docker is unavailable"}</span>
          )}
        </div>
      </section>

      {/* Background / system tray */}
      <section className="rounded-xl border border-zinc-800 bg-zinc-950/60 p-4">
        <div className="flex items-center justify-between">
          <h2 className="text-xs font-semibold uppercase tracking-wide text-zinc-500">Background</h2>
          <button
            role="switch"
            aria-checked={runInBackground}
            aria-label="Keep LocalKit running in the system tray"
            onClick={toggleRunInBackground}
            className={`relative inline-flex h-6 w-11 items-center rounded-full transition-colors ${
              runInBackground ? "bg-violet-600" : "bg-zinc-700"
            }`}
          >
            <span
              className={`inline-block h-4 w-4 transform rounded-full bg-white transition-transform ${
                runInBackground ? "translate-x-6" : "translate-x-1"
              }`}
            />
          </button>
        </div>
        <p className="mt-2.5 text-sm text-zinc-500">
          Keep LocalKit running in the system tray when the window is closed. Your sites keep
          running in the background; reopen the window or quit from the tray icon. Running sites
          keep serving even after quitting the app — stop them from the dashboard or the tray
          menu.
        </p>
      </section>

      {/* Paths + defaults */}
      <section className="rounded-xl border border-zinc-800 bg-zinc-950/60 p-4">
        <h2 className="text-xs font-semibold uppercase tracking-wide text-zinc-500">Local environment</h2>
        <dl className="mt-2.5 space-y-2 text-sm">
          <div className="flex items-center justify-between gap-4">
            <dt className="text-zinc-500">Data directory</dt>
            <dd className="truncate font-mono text-xs text-zinc-300">{info?.data_dir ?? "…"}</dd>
          </div>
          <div className="flex items-center justify-between gap-4">
            <dt className="text-zinc-500">Sites directory</dt>
            <dd className="truncate font-mono text-xs text-zinc-300">{info?.sites_dir ?? "…"}</dd>
          </div>
          <div className="flex items-center justify-between gap-4">
            <dt className="text-zinc-500">WordPress versions</dt>
            <dd className="font-mono text-xs text-zinc-300">{info?.wp_versions.join(", ") ?? "…"}</dd>
          </div>
          <div className="flex items-center justify-between gap-4">
            <dt className="text-zinc-500">PHP versions</dt>
            <dd className="font-mono text-xs text-zinc-300">{info?.php_versions.join(", ") ?? "…"}</dd>
          </div>
        </dl>
      </section>
    </div>
  );
}

const FONT_SIZES = [11, 12, 13, 14, 15, 16];
const SCROLLBACKS: { label: string; value: number }[] = [
  { label: "1k", value: 1000 },
  { label: "5k", value: 5000 },
  { label: "10k", value: 10000 },
];

function TerminalSection() {
  const [fontSize, setFontSize] = useTerminalFontSize();
  const [scrollback, setScrollback] = useTerminalScrollback();

  return (
    <div className="space-y-4">
      {/* Font size — live-applies to open terminals */}
      <section className="rounded-xl border border-zinc-800 bg-zinc-950/60 p-4">
        <h2 className="text-xs font-semibold uppercase tracking-wide text-zinc-500">Font size</h2>
        <div className="mt-2.5 flex items-center gap-1">
          {FONT_SIZES.map((px) => (
            <button
              key={px}
              onClick={() => setFontSize(px)}
              aria-pressed={fontSize === px}
              className={`rounded-md px-2.5 py-1 text-xs font-medium tabular-nums transition-colors ${
                fontSize === px
                  ? "bg-violet-500/15 text-violet-400"
                  : "text-zinc-400 hover:bg-zinc-900 hover:text-zinc-100"
              }`}
            >
              {px}
            </button>
          ))}
          <span className="ml-2 text-xs text-zinc-600">px</span>
        </div>
        <p className="mt-2.5 text-sm text-zinc-500">
          Applies immediately to every open terminal.
        </p>
      </section>

      {/* Scrollback — new terminals only */}
      <section className="rounded-xl border border-zinc-800 bg-zinc-950/60 p-4">
        <h2 className="text-xs font-semibold uppercase tracking-wide text-zinc-500">
          Scrollback lines
        </h2>
        <div className="mt-2.5 flex items-center gap-1">
          {SCROLLBACKS.map((opt) => (
            <button
              key={opt.value}
              onClick={() => setScrollback(opt.value)}
              aria-pressed={scrollback === opt.value}
              className={`rounded-md px-2.5 py-1 text-xs font-medium tabular-nums transition-colors ${
                scrollback === opt.value
                  ? "bg-violet-500/15 text-violet-400"
                  : "text-zinc-400 hover:bg-zinc-900 hover:text-zinc-100"
              }`}
            >
              {opt.label}
            </button>
          ))}
        </div>
        <p className="mt-2.5 text-sm text-zinc-500">
          Applies to newly opened terminals — reconnect or open a new tab to pick it up.
        </p>
      </section>

      {/* Behavior notes — hardcoded for v1, documented here */}
      <section className="rounded-xl border border-zinc-800 bg-zinc-950/60 p-4">
        <h2 className="text-xs font-semibold uppercase tracking-wide text-zinc-500">Behavior</h2>
        <ul className="mt-2.5 list-disc space-y-1.5 pl-4 text-sm text-zinc-500">
          <li>Selecting text copies it to the clipboard automatically.</li>
          <li>URLs in the output are Ctrl+click to open in your browser.</li>
          <li>
            As you type, a matching previous command appears as ghost text — press → or End to
            accept it. History is kept per site on this machine.
          </li>
        </ul>
      </section>
    </div>
  );
}
