import { useEffect } from "react";
import { keyCombo } from "../lib/shortcuts";
import { effectiveCombo } from "../lib/keybindings";
import { buildCommands } from "../lib/commands";
import { useSettings } from "../stores/settings";

/**
 * Global shortcut dispatcher (plan 15, ported from Faro): one window keydown
 * listener matching `keyCombo(e)` against the commands' effective bindings.
 *
 * Correctness rule — the editable-target guard: shortcuts never fire while
 * typing in an input/textarea/select/contenteditable or the xterm helper
 * textarea, unless the combo has `mod` (mod-combos stay global, e.g. mod+K
 * from inside the palette's own search field).
 */

function isEditableTarget(target: EventTarget | null): boolean {
  return (
    target instanceof Element &&
    !!target.closest("input, textarea, select, [contenteditable], .xterm-helper-textarea")
  );
}

function inTerminal(target: EventTarget | null): boolean {
  return target instanceof Element && !!target.closest(".xterm");
}

export function useShortcuts() {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const combo = keyCombo(e);
      if (!combo) return;
      if (isEditableTarget(e.target) && !combo.includes("mod")) return;

      const values = useSettings.getState().values;
      const ctx = inTerminal(e.target) ? "terminal" : "global";
      const cmd = buildCommands().find(
        (c) =>
          effectiveCombo(c, values) === combo &&
          ((c.context ?? "global") === "global" || ctx === "terminal")
      );
      if (cmd) {
        e.preventDefault();
        cmd.run();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);
}
