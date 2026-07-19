import { useCallback, useEffect, useState } from "react";
import { ipc } from "../lib/ipc";
import type { AppInfo, DockerStatus } from "../lib/types";
import ServerKitSettings from "../components/ServerKitSettings";

export default function Settings() {
  const [docker, setDocker] = useState<DockerStatus | null>(null);
  const [checking, setChecking] = useState(false);
  const [info, setInfo] = useState<AppInfo | null>(null);

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
  }, [checkDocker]);

  return (
    <div className="p-8">
      <h1 className="text-2xl font-semibold text-white">Settings</h1>

      <div className="mt-6 grid max-w-3xl grid-cols-1 gap-4">
        {/* Docker */}
        <section className="rounded-xl border border-zinc-800 bg-zinc-900/60 p-5">
          <div className="flex items-center justify-between">
            <h2 className="text-sm font-semibold uppercase tracking-wide text-zinc-500">Docker</h2>
            <button onClick={() => void checkDocker()} className="text-xs text-zinc-500 hover:text-zinc-300">
              {checking ? "Checking…" : "Re-check"}
            </button>
          </div>
          <div className="mt-3 flex items-center gap-2 text-sm">
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

        {/* Paths + defaults */}
        <section className="rounded-xl border border-zinc-800 bg-zinc-900/60 p-5">
          <h2 className="text-sm font-semibold uppercase tracking-wide text-zinc-500">Local environment</h2>
          <dl className="mt-3 space-y-2 text-sm">
            <div className="flex items-center justify-between gap-4">
              <dt className="text-zinc-500">Data directory</dt>
              <dd className="truncate font-mono text-zinc-300">{info?.data_dir ?? "…"}</dd>
            </div>
            <div className="flex items-center justify-between gap-4">
              <dt className="text-zinc-500">Sites directory</dt>
              <dd className="truncate font-mono text-zinc-300">{info?.sites_dir ?? "…"}</dd>
            </div>
            <div className="flex items-center justify-between gap-4">
              <dt className="text-zinc-500">WordPress versions</dt>
              <dd className="font-mono text-zinc-300">{info?.wp_versions.join(", ") ?? "…"}</dd>
            </div>
            <div className="flex items-center justify-between gap-4">
              <dt className="text-zinc-500">PHP versions</dt>
              <dd className="font-mono text-zinc-300">{info?.php_versions.join(", ") ?? "…"}</dd>
            </div>
          </dl>
        </section>

        {/* ServerKit connections (M3, read-only) */}
        <ServerKitSettings />
      </div>
    </div>
  );
}
