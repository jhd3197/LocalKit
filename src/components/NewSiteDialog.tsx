import { useEffect, useState } from "react";
import { ipc } from "../lib/ipc";
import { useNav } from "../stores/nav";
import { useSites } from "../stores/sites";
import type { AppInfo } from "../lib/types";

export default function NewSiteDialog({ onClose }: { onClose: () => void }) {
  const createSite = useSites((s) => s.createSite);
  const creating = useSites((s) => s.creating);
  const navigate = useNav((s) => s.navigate);

  const [name, setName] = useState("");
  const [wpVersion, setWpVersion] = useState("");
  const [phpVersion, setPhpVersion] = useState("");
  const [versions, setVersions] = useState<Pick<AppInfo, "wp_versions" | "php_versions">>({
    wp_versions: ["6.7", "6.6", "6.5"],
    php_versions: ["8.3", "8.2", "8.1"],
  });
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    ipc
      .appInfo()
      .then((info) => setVersions(info))
      .catch(() => {});
  }, []);

  useEffect(() => {
    setWpVersion((v) => v || versions.wp_versions[0]);
    setPhpVersion((v) => v || versions.php_versions[0]);
  }, [versions]);

  const submit = async () => {
    setError(null);
    try {
      const site = await createSite(name, wpVersion, phpVersion);
      onClose();
      navigate({ name: "site", id: site.id });
    } catch (e) {
      setError(typeof e === "string" ? e : String(e));
    }
  };

  return (
    <div className="fixed inset-0 z-40 flex items-center justify-center bg-black/60" onClick={onClose}>
      <div
        className="w-full max-w-md rounded-xl border border-zinc-800 bg-zinc-900 p-6 shadow-2xl"
        onClick={(e) => e.stopPropagation()}
      >
        <h2 className="text-lg font-semibold text-white">New WordPress site</h2>
        <p className="mt-1 text-sm text-zinc-500">
          LocalKit will create a Docker project, start it, and install WordPress automatically.
        </p>

        <div className="mt-5 flex flex-col gap-4">
          <label className="block">
            <span className="mb-1 block text-sm text-zinc-400">Site name</span>
            <input
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="My Blog"
              autoFocus
              className="w-full rounded-md border border-zinc-700 bg-zinc-950 px-3 py-2 text-sm text-zinc-100 outline-none focus:border-violet-600"
            />
          </label>

          <div className="grid grid-cols-2 gap-4">
            <label className="block">
              <span className="mb-1 block text-sm text-zinc-400">WordPress</span>
              <select
                value={wpVersion}
                onChange={(e) => setWpVersion(e.target.value)}
                className="w-full rounded-md border border-zinc-700 bg-zinc-950 px-3 py-2 text-sm text-zinc-100 outline-none focus:border-violet-600"
              >
                {versions.wp_versions.map((v) => (
                  <option key={v} value={v}>
                    {v}
                  </option>
                ))}
              </select>
            </label>
            <label className="block">
              <span className="mb-1 block text-sm text-zinc-400">PHP</span>
              <select
                value={phpVersion}
                onChange={(e) => setPhpVersion(e.target.value)}
                className="w-full rounded-md border border-zinc-700 bg-zinc-950 px-3 py-2 text-sm text-zinc-100 outline-none focus:border-violet-600"
              >
                {versions.php_versions.map((v) => (
                  <option key={v} value={v}>
                    {v}
                  </option>
                ))}
              </select>
            </label>
          </div>

          {error && <p className="text-sm text-red-400">{error}</p>}
        </div>

        <div className="mt-6 flex justify-end gap-2">
          <button
            onClick={onClose}
            disabled={creating}
            className="rounded-md px-4 py-2 text-sm text-zinc-400 hover:text-zinc-200 disabled:opacity-50"
          >
            Cancel
          </button>
          <button
            onClick={submit}
            disabled={creating || !name.trim()}
            className="rounded-md bg-violet-600 px-4 py-2 text-sm font-medium text-white hover:bg-violet-500 disabled:opacity-50"
          >
            {creating ? "Creating…" : "Create site"}
          </button>
        </div>
      </div>
    </div>
  );
}
