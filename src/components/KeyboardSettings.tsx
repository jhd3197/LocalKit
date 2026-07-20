import { useState } from "react";
import { bindableCommands, type Command } from "../lib/commands";
import { effectiveCombo, findConflict, hasOverride, SHORTCUT_PREFIX, UNBOUND } from "../lib/keybindings";
import { comboLabel, keyCombo } from "../lib/shortcuts";
import { fuzzyFilter } from "../lib/fuzzy";
import { useNav } from "../stores/nav";
import { useSettings } from "../stores/settings";

/**
 * Settings → Keyboard (plan 15, phase 3 — ported from Faro): every static
 * command with a click-to-record capture field. Esc cancels, Backspace clears
 * back to the default binding, conflicts are caught with overwrite/cancel.
 * Overrides persist in the app_settings KV as `shortcut.<commandId>`.
 */
export default function KeyboardSettings() {
  const values = useSettings((s) => s.values);
  const set = useSettings((s) => s.set);
  const remove = useSettings((s) => s.remove);
  const setCheatsheetOpen = useNav((s) => s.setCheatsheetOpen);

  const [query, setQuery] = useState("");
  const [recording, setRecording] = useState<string | null>(null);
  const [conflict, setConflict] = useState<{ cmdId: string; combo: string; other: Command } | null>(
    null
  );

  const commands = bindableCommands();
  const filtered = fuzzyFilter(commands, query, (c) => `${c.group} ${c.title}`);

  const assign = (cmdId: string, combo: string) => set(SHORTCUT_PREFIX + cmdId, combo);
  const resetAll = () => {
    for (const key of Object.keys(values)) {
      if (key.startsWith(SHORTCUT_PREFIX)) remove(key);
    }
  };

  const onCaptureKey = (e: React.KeyboardEvent, cmd: Command) => {
    e.preventDefault();
    e.stopPropagation();
    if (e.key === "Escape") {
      setRecording(null);
      return;
    }
    if (e.key === "Backspace" || e.key === "Delete") {
      remove(SHORTCUT_PREFIX + cmd.id); // back to the default binding
      setRecording(null);
      return;
    }
    const combo = keyCombo(e.nativeEvent);
    if (!combo) return; // modifier-only press — keep recording
    const other = findConflict(commands, combo, values, cmd.id);
    if (other) {
      setConflict({ cmdId: cmd.id, combo, other });
    } else {
      assign(cmd.id, combo);
    }
    setRecording(null);
  };

  let lastGroup: string | null = null;

  return (
    <div className="space-y-4">
      <section className="rounded-xl border border-zinc-800 bg-zinc-950/60 p-4">
        <div className="flex items-center justify-between gap-3">
          <h2 className="text-xs font-semibold uppercase tracking-wide text-zinc-500">Shortcuts</h2>
          <div className="flex items-center gap-3">
            <button
              onClick={() => setCheatsheetOpen(true)}
              className="text-xs text-zinc-500 hover:text-zinc-300"
            >
              View cheat-sheet
            </button>
            <button onClick={resetAll} className="text-xs text-zinc-500 hover:text-zinc-300">
              Reset all to defaults
            </button>
          </div>
        </div>

        <input
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Filter commands…"
          className="mt-3 w-full rounded-md border border-zinc-700 bg-zinc-950 px-3 py-1.5 text-sm text-zinc-100 outline-none placeholder:text-zinc-600 focus:border-violet-600"
        />

        <div className="mt-2">
          {filtered.length === 0 && (
            <p className="py-4 text-center text-sm text-zinc-600">No matching commands.</p>
          )}
          {filtered.map((cmd) => {
            const header =
              cmd.group !== lastGroup ? (
                <div className="px-1 pb-1 pt-3 text-[11px] font-medium uppercase tracking-wide text-zinc-600">
                  {cmd.group}
                </div>
              ) : null;
            lastGroup = cmd.group;
            const combo = effectiveCombo(cmd, values);
            const isRecording = recording === cmd.id;
            const activeConflict = conflict?.cmdId === cmd.id ? conflict : null;
            return (
              <div key={cmd.id}>
                {header}
                <div className="flex items-center justify-between gap-3 rounded-md px-1 py-1.5">
                  <span className="text-sm text-zinc-300">{cmd.title}</span>
                  <span className="flex items-center gap-1.5">
                    {hasOverride(cmd, values) && (
                      <button
                        onClick={() => remove(SHORTCUT_PREFIX + cmd.id)}
                        title="Reset to default"
                        aria-label={`Reset ${cmd.title} to default`}
                        className="rounded px-1 text-xs text-zinc-600 hover:text-zinc-300"
                      >
                        ↺
                      </button>
                    )}
                    <button
                      onClick={() => {
                        setConflict(null);
                        setRecording(isRecording ? null : cmd.id);
                      }}
                      onKeyDown={(e) => isRecording && onCaptureKey(e, cmd)}
                      className={`min-w-[4.5rem] rounded-md border px-2 py-1 font-mono text-xs ${
                        isRecording
                          ? "border-violet-600 text-violet-300"
                          : "border-zinc-700 text-zinc-400 hover:border-zinc-500 hover:text-zinc-200"
                      }`}
                    >
                      {isRecording ? "Press keys…" : combo ? comboLabel(combo) : "Not bound"}
                    </button>
                  </span>
                </div>
                {activeConflict && (
                  <div className="mx-1 mb-1.5 flex items-center justify-between gap-2 rounded-md border border-amber-900/60 bg-amber-950/40 px-2.5 py-1.5 text-xs">
                    <span className="text-amber-200">
                      {comboLabel(activeConflict.combo)} is already used by “
                      {activeConflict.other.title}”
                    </span>
                    <span className="flex shrink-0 gap-1.5">
                      <button
                        onClick={() => {
                          set(SHORTCUT_PREFIX + activeConflict.other.id, UNBOUND);
                          assign(activeConflict.cmdId, activeConflict.combo);
                          setConflict(null);
                        }}
                        className="rounded border border-amber-800 px-2 py-0.5 text-amber-200 hover:border-amber-600"
                      >
                        Overwrite
                      </button>
                      <button
                        onClick={() => setConflict(null)}
                        className="rounded px-2 py-0.5 text-zinc-400 hover:text-zinc-200"
                      >
                        Cancel
                      </button>
                    </span>
                  </div>
                )}
              </div>
            );
          })}
        </div>

        <p className="mt-3 text-sm text-zinc-500">
          Click a binding and press the new keys. Esc cancels, Backspace restores the default.
          Shortcuts never fire while typing in a field or the terminal.
        </p>
      </section>
    </div>
  );
}
