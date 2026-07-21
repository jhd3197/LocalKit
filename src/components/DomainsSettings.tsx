import { useEffect, useState } from "react";
import { useRouter } from "../stores/router";
import {
  DEFAULT_HTTP_PORT,
  DEFAULT_HTTPS_PORT,
  FALLBACK_HTTP_PORT,
  FALLBACK_HTTPS_PORT,
  isDefaultPorts,
} from "../lib/domains";
import type { PortConflict } from "../lib/types";

/** "port 80 (httpd.exe) and port 443 (httpd.exe)" */
export function describeConflicts(conflicts: PortConflict[]): string {
  return conflicts
    .map((c) => `port ${c.port}${c.process ? ` (${c.process})` : ""}`)
    .join(" and ");
}

export default function DomainsSettings() {
  const status = useRouter((s) => s.status);
  const busy = useRouter((s) => s.busy);
  const refresh = useRouter((s) => s.refresh);
  const setEnabled = useRouter((s) => s.setEnabled);
  const setPorts = useRouter((s) => s.setPorts);
  const trustCa = useRouter((s) => s.trustCa);
  const [actionError, setActionError] = useState<string | null>(null);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const enabled = status?.enabled ?? false;
  const running = status?.running ?? false;
  const caTrusted = status?.ca_trusted ?? false;
  const conflicts = status?.conflicts ?? [];
  const httpPort = status?.http_port ?? DEFAULT_HTTP_PORT;
  const httpsPort = status?.https_port ?? DEFAULT_HTTPS_PORT;
  const onDefaults = isDefaultPorts(status);

  const run = async (fn: () => Promise<string | null>) => {
    setActionError(null);
    const err = await fn();
    if (err) setActionError(err);
  };

  /** One click out of the conflict: move to 8080/8443 and retry enabling. */
  const useFallbackPorts = () =>
    run(async () => {
      const err = await setPorts(FALLBACK_HTTP_PORT, FALLBACK_HTTPS_PORT);
      if (err) return err;
      if (!useRouter.getState().status?.enabled) await setEnabled(true);
      return null;
    });

  return (
    <section className="rounded-xl border border-zinc-800 bg-zinc-950/60 p-4">
      <div className="flex items-center justify-between">
        <h2 className="text-xs font-semibold uppercase tracking-wide text-zinc-500">Local domains</h2>
        {/* Enable toggle */}
        <button
          role="switch"
          aria-checked={enabled}
          aria-label="Enable local domains"
          disabled={busy}
          onClick={() => void setEnabled(!enabled)}
          className={`relative inline-flex h-6 w-11 items-center rounded-full transition-colors disabled:opacity-50 ${
            enabled ? "bg-violet-600" : "bg-zinc-700"
          }`}
        >
          <span
            className={`inline-block h-4 w-4 transform rounded-full bg-white transition-transform ${
              enabled ? "translate-x-6" : "translate-x-1"
            }`}
          />
        </button>
      </div>

      <p className="mt-2.5 text-sm text-zinc-500">
        Reach sites as{" "}
        <code className="font-mono text-xs text-zinc-300">http://&lt;site&gt;.test</code> instead
        of <code className="font-mono text-xs">localhost:&lt;port&gt;</code>. Enabling asks for
        one-time administrator approval so LocalKit can manage a small marked block in your hosts
        file. <code className="font-mono text-xs">.test</code> is reserved for testing (RFC 2606)
        and never collides with LocalWP's <code className="font-mono text-xs">.local</code>{" "}
        domains. A small Caddy router container listens on ports {httpPort}/{httpsPort} while
        enabled.
      </p>

      {/* Router status */}
      <div className="mt-3 flex items-center gap-2 text-sm">
        <span
          className={`h-2.5 w-2.5 rounded-full ${
            status === null
              ? "bg-zinc-600"
              : !enabled
                ? "bg-zinc-600"
                : running
                  ? "bg-emerald-400"
                  : conflicts.length > 0
                    ? "bg-amber-400"
                    : "bg-red-400"
          }`}
        />
        {status === null ? (
          <span className="text-zinc-500">Checking router status…</span>
        ) : conflicts.length > 0 ? (
          // Before the enabled check: a failed *enable* leaves the flag off
          // (the backend short-circuits before setting it), and "Local domains
          // are off" directly above an amber conflict callout reads as if
          // nothing happened.
          <span className="text-amber-300">Router is blocked by another program</span>
        ) : !enabled ? (
          <span className="text-zinc-500">
            Local domains are off — sites use <code className="font-mono text-xs">localhost:&lt;port&gt;</code>.
          </span>
        ) : running ? (
          <span className="text-zinc-300">
            Router is running on ports {httpPort}/{httpsPort}
            {!onDefaults && <span className="text-zinc-500"> (fallback)</span>}
          </span>
        ) : (
          <span className="text-red-300">Router is not running</span>
        )}
      </div>

      {/* Port conflict — the LocalWP case. Named cause + two ways out, so the
          user is never left staring at a foreign 404 with no explanation. */}
      {conflicts.length > 0 && (
        <div className="mt-3 rounded-lg border border-amber-900/60 bg-amber-950/40 p-3.5">
          <p className="text-sm font-medium text-amber-200">
            Another program is using {describeConflicts(conflicts)}
          </p>
          <p className="mt-1 text-xs text-amber-200/80">
            Local domains need those ports. Only one program can own them at a time — LocalWP's
            router, IIS, Skype and other web servers are the usual culprits. Quit it and retry, or
            run LocalKit's router on fallback ports instead (your sites become{" "}
            <code className="font-mono">http://&lt;site&gt;.test:{FALLBACK_HTTP_PORT}</code>).
          </p>
          <div className="mt-2.5 flex flex-wrap gap-2">
            {onDefaults && (
              <button
                onClick={() => void useFallbackPorts()}
                disabled={busy}
                className="rounded-md bg-violet-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-violet-500 disabled:opacity-50"
              >
                {busy ? "Working…" : `Use fallback ports (${FALLBACK_HTTP_PORT}/${FALLBACK_HTTPS_PORT})`}
              </button>
            )}
            <button
              onClick={() => void run(async () => (await setEnabled(true), null))}
              disabled={busy}
              className="rounded-md border border-amber-800 px-3 py-1.5 text-xs font-medium text-amber-200 hover:border-amber-600 disabled:opacity-50"
            >
              Retry
            </button>
          </div>
        </div>
      )}

      {status?.error && conflicts.length === 0 && (
        <p className="mt-2 whitespace-pre-line text-sm text-red-400">{status.error}</p>
      )}
      {actionError && <p className="mt-2 text-sm text-red-400">{actionError}</p>}

      <RouterPortFields
        http={httpPort}
        https={httpsPort}
        busy={busy}
        onApply={(h, s) => run(() => setPorts(h, s))}
      />

      {/* HTTPS trust — only meaningful on the standard 443, since a
          non-standard https port re-prompts for a cert exception anyway. */}
      {enabled && running && onDefaults && (
        <div className="mt-4 rounded-lg border border-zinc-800 bg-zinc-900/60 p-3.5">
          <div className="flex items-center justify-between gap-3">
            <div className="min-w-0">
              <p className="text-sm font-medium text-zinc-200">HTTPS</p>
              <p className="mt-0.5 text-xs text-zinc-500">
                {caTrusted
                  ? "The LocalKit CA is trusted — https URLs open without browser warnings."
                  : "Trust the router's local CA to open https:// URLs without browser warnings. Until then, plain http:// works fine."}
              </p>
            </div>
            {caTrusted ? (
              <span className="shrink-0 rounded-full border border-emerald-800 bg-emerald-500/15 px-2.5 py-0.5 text-xs font-medium text-emerald-400">
                Trusted
              </span>
            ) : (
              <button
                onClick={() => void run(trustCa)}
                disabled={busy}
                className="shrink-0 rounded-md bg-violet-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-violet-500 disabled:opacity-50"
              >
                {busy ? "Working…" : "Trust HTTPS certificate"}
              </button>
            )}
          </div>
        </div>
      )}
    </section>
  );
}

/** Two validated number fields; Apply is enabled only for a valid change. */
function RouterPortFields({
  http,
  https,
  busy,
  onApply,
}: {
  http: number;
  https: number;
  busy: boolean;
  onApply: (http: number, https: number) => void;
}) {
  const [draftHttp, setDraftHttp] = useState(String(http));
  const [draftHttps, setDraftHttps] = useState(String(https));

  // Re-sync when the backend changes the ports under us (fallback one-click).
  useEffect(() => {
    setDraftHttp(String(http));
    setDraftHttps(String(https));
  }, [http, https]);

  const h = Number(draftHttp);
  const s = Number(draftHttps);
  const valid =
    Number.isInteger(h) && h > 0 && h < 65536 && Number.isInteger(s) && s > 0 && s < 65536 && h !== s;
  const changed = h !== http || s !== https;
  const invalidReason =
    !Number.isInteger(h) || h <= 0 || h > 65535 || !Number.isInteger(s) || s <= 0 || s > 65535
      ? "Ports must be between 1 and 65535."
      : h === s
        ? "The HTTP and HTTPS ports must be different."
        : null;

  return (
    <div className="mt-4 rounded-lg border border-zinc-800 bg-zinc-900/60 p-3.5">
      <p className="text-sm font-medium text-zinc-200">Router ports</p>
      <p className="mt-0.5 text-xs text-zinc-500">
        Host ports the router listens on. {DEFAULT_HTTP_PORT}/{DEFAULT_HTTPS_PORT} give clean{" "}
        <code className="font-mono">http://&lt;site&gt;.test</code> URLs; any other pair appends the
        port. Changing these restarts the router and updates each running site's WordPress URL.
      </p>
      <div className="mt-2.5 flex flex-wrap items-end gap-3">
        <label className="text-xs text-zinc-500">
          HTTP
          <input
            type="number"
            min={1}
            max={65535}
            value={draftHttp}
            onChange={(e) => setDraftHttp(e.target.value)}
            className="mt-1 block w-24 rounded-md border border-zinc-700 bg-zinc-950 px-2 py-1 font-mono text-sm tabular-nums text-zinc-100 focus:border-violet-500 focus:outline-none"
          />
        </label>
        <label className="text-xs text-zinc-500">
          HTTPS
          <input
            type="number"
            min={1}
            max={65535}
            value={draftHttps}
            onChange={(e) => setDraftHttps(e.target.value)}
            className="mt-1 block w-24 rounded-md border border-zinc-700 bg-zinc-950 px-2 py-1 font-mono text-sm tabular-nums text-zinc-100 focus:border-violet-500 focus:outline-none"
          />
        </label>
        <button
          onClick={() => onApply(h, s)}
          disabled={busy || !valid || !changed}
          className="rounded-md border border-zinc-700 px-3 py-1.5 text-xs font-medium text-zinc-200 hover:border-zinc-500 disabled:opacity-40"
        >
          Apply
        </button>
      </div>
      {changed && invalidReason && <p className="mt-2 text-xs text-red-400">{invalidReason}</p>}
    </div>
  );
}
