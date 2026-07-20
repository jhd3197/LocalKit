import { useEffect, useMemo, useRef, useState } from "react";
import { useCommands, type Command } from "../lib/commands";
import { effectiveCombo } from "../lib/keybindings";
import { comboLabel } from "../lib/shortcuts";
import { fuzzyFilter } from "../lib/fuzzy";
import { useDialog } from "../hooks/useDialog";
import { useNav } from "../stores/nav";
import { useSettings } from "../stores/settings";

/**
 * Fuzzy command palette (plan 15, phase 2 — ported from Faro). mod+K opens
 * (registered as the `toggle-palette` shortcut in App), ↑/↓ + Enter run,
 * Esc / outside-click closes. Pure frontend over the sites store — mock mode
 * needs nothing.
 */
export default function CommandPalette() {
  const open = useNav((s) => s.paletteOpen);
  if (!open) return null;
  return <PalettePanel />;
}

function PalettePanel() {
  const setOpen = useNav((s) => s.setPaletteOpen);
  const commands = useCommands();
  const values = useSettings((s) => s.values);
  const [query, setQuery] = useState("");
  const [index, setIndex] = useState(0);
  const listRef = useRef<HTMLDivElement>(null);
  const { overlayProps, panelProps } = useDialog(() => setOpen(false));

  const close = () => setOpen(false);

  const filtered = useMemo(
    () => fuzzyFilter(commands, query, (c) => `${c.group} ${c.title}`),
    [commands, query]
  );
  const active = Math.min(index, Math.max(filtered.length - 1, 0));

  // Keep the highlighted row visible while arrowing through the list.
  useEffect(() => {
    listRef.current
      ?.querySelector(`[data-idx="${active}"]`)
      ?.scrollIntoView({ block: "nearest" });
  }, [active]);

  const runCommand = (cmd: Command) => {
    close();
    cmd.run();
  };

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setIndex((i) => Math.min(i + 1, filtered.length - 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setIndex((i) => Math.max(i - 1, 0));
    } else if (e.key === "Enter") {
      e.preventDefault();
      const cmd = filtered[active];
      if (cmd) runCommand(cmd);
    }
  };

  // Group headers: show when the group name changes between consecutive rows.
  let lastGroup: string | null = null;

  return (
    <div
      {...overlayProps}
      className="fixed inset-0 z-50 flex items-start justify-center bg-black/60 pt-[15vh] backdrop-blur-sm"
    >
      <div
        {...panelProps}
        role="dialog"
        aria-modal="true"
        aria-label="Command palette"
        className="w-[34rem] max-w-[92vw] overflow-hidden rounded-xl border border-zinc-800 bg-zinc-900 shadow-panel"
      >
        <input
          autoFocus
          value={query}
          onChange={(e) => {
            setQuery(e.target.value);
            setIndex(0);
          }}
          onKeyDown={onKeyDown}
          placeholder="Type a command or search…"
          className="w-full border-b border-zinc-800 bg-transparent px-4 py-3 text-sm text-zinc-100 outline-none placeholder:text-zinc-600"
        />
        <div ref={listRef} className="max-h-[50vh] overflow-y-auto p-1.5">
          {filtered.length === 0 && (
            <p className="px-3 py-6 text-center text-sm text-zinc-600">No matching commands.</p>
          )}
          {filtered.map((cmd, i) => {
            const header =
              cmd.group !== lastGroup ? (
                <div className="px-2.5 pb-1 pt-2 text-[11px] font-medium uppercase tracking-wide text-zinc-600">
                  {cmd.group}
                </div>
              ) : null;
            lastGroup = cmd.group;
            const combo = effectiveCombo(cmd, values);
            return (
              <div key={cmd.id}>
                {header}
                <button
                  data-idx={i}
                  onMouseEnter={() => setIndex(i)}
                  onClick={() => runCommand(cmd)}
                  className={`flex w-full items-center justify-between rounded-md px-2.5 py-1.5 text-left text-sm ${
                    i === active ? "bg-violet-500/15 text-violet-300" : "text-zinc-300"
                  }`}
                >
                  <span className="truncate">{cmd.title}</span>
                  {combo && (
                    <kbd className="ml-3 shrink-0 rounded border border-zinc-700 bg-zinc-950 px-1.5 py-0.5 font-mono text-[10px] text-zinc-500">
                      {comboLabel(combo)}
                    </kbd>
                  )}
                </button>
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}
