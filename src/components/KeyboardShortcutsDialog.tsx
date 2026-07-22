import { bindableCommands, type Command } from "../lib/commands";
import { effectiveCombo } from "../lib/keybindings";
import { comboLabel } from "../lib/shortcuts";
import { useDialog } from "../hooks/useDialog";
import { useNav } from "../stores/nav";
import { useSettings } from "../stores/settings";
import { CloseIcon } from "./icons";

/**
 * Keyboard-shortcuts cheat-sheet (plan 15 — ported from Faro's
 * KeyboardShortcutsDialog): every command with an effective binding, grouped.
 * Opened with `?` or from Settings → Keyboard. Reads the same resolver as the
 * dispatcher, so user overrides show up here automatically.
 */
export default function KeyboardShortcutsDialog() {
  const open = useNav((s) => s.cheatsheetOpen);
  if (!open) return null;
  return <CheatsheetPanel />;
}

function CheatsheetPanel() {
  const setOpen = useNav((s) => s.setCheatsheetOpen);
  const values = useSettings((s) => s.values);
  const { overlayProps, panelProps } = useDialog(() => setOpen(false));

  const bound = bindableCommands()
    .map((cmd) => ({ cmd, combo: effectiveCombo(cmd, values) }))
    .filter((x): x is { cmd: Command; combo: string } => !!x.combo);

  let lastGroup: string | null = null;

  return (
    <div
      {...overlayProps}
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm"
    >
      <div
        {...panelProps}
        role="dialog"
        aria-modal="true"
        aria-label="Keyboard shortcuts"
        className="w-[26rem] max-w-[92vw] overflow-hidden rounded-xl border border-zinc-800 bg-zinc-900 shadow-panel"
      >
        <div className="flex items-center border-b border-zinc-800 px-5 py-3">
          <span className="text-sm font-semibold text-zinc-50">Keyboard shortcuts</span>
          <div className="flex-1" />
          <button
            onClick={() => setOpen(false)}
            aria-label="Close"
            className="rounded-md p-1.5 text-zinc-500 hover:bg-zinc-800 hover:text-zinc-300"
          >
            <CloseIcon className="h-3.5 w-3.5" />
          </button>
        </div>
        <div className="max-h-[60vh] overflow-y-auto px-5 py-3">
          {bound.map(({ cmd, combo }) => {
            const header =
              cmd.group !== lastGroup ? (
                <div className="pb-1 pt-2.5 text-[11px] font-medium uppercase tracking-wide text-zinc-600 first:pt-0">
                  {cmd.group}
                </div>
              ) : null;
            lastGroup = cmd.group;
            return (
              <div key={cmd.id}>
                {header}
                <div className="flex items-center justify-between py-1.5">
                  <span className="text-sm text-zinc-300">{cmd.title}</span>
                  <kbd className="rounded border border-zinc-700 bg-zinc-950 px-1.5 py-0.5 font-mono text-[10px] text-zinc-500">
                    {comboLabel(combo)}
                  </kbd>
                </div>
              </div>
            );
          })}
        </div>
        <p className="border-t border-zinc-800 px-5 py-2.5 text-xs text-zinc-600">
          Rebind any shortcut in Settings → Keyboard.
        </p>
      </div>
    </div>
  );
}
