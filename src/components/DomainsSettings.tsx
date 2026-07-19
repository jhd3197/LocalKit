import { useEffect, useState } from "react";
import { useRouter } from "../stores/router";

export default function DomainsSettings() {
  const status = useRouter((s) => s.status);
  const busy = useRouter((s) => s.busy);
  const refresh = useRouter((s) => s.refresh);
  const setEnabled = useRouter((s) => s.setEnabled);
  const trustCa = useRouter((s) => s.trustCa);
  const [trustError, setTrustError] = useState<string | null>(null);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const enabled = status?.enabled ?? false;
  const running = status?.running ?? false;
  const caTrusted = status?.ca_trusted ?? false;

  const doTrust = async () => {
    setTrustError(null);
    const err = await trustCa();
    if (err) setTrustError(err);
  };

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
        domains. A small Caddy router container listens on ports 80/443 while enabled.
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
                  : "bg-red-400"
          }`}
        />
        {status === null ? (
          <span className="text-zinc-500">Checking router status…</span>
        ) : !enabled ? (
          <span className="text-zinc-500">
            Local domains are off — sites use <code className="font-mono text-xs">localhost:&lt;port&gt;</code>.
          </span>
        ) : running ? (
          <span className="text-zinc-300">Router is running on ports 80/443</span>
        ) : (
          <span className="text-red-300">Router is not running</span>
        )}
      </div>

      {status?.error && <p className="mt-2 whitespace-pre-line text-sm text-red-400">{status.error}</p>}
      {trustError && <p className="mt-2 text-sm text-red-400">{trustError}</p>}

      {/* HTTPS trust */}
      {enabled && running && (
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
                onClick={() => void doTrust()}
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
