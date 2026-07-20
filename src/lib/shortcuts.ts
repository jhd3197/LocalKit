/**
 * Keyboard combo canonicalizer + display labels (ported from Faro).
 *
 * A combo is a `+`-joined string of modifiers plus one key, e.g. `mod+k`,
 * `shift+?`, `alt+arrowup`. `mod` = Ctrl on Windows/Linux, Cmd on macOS, so
 * bindings are written once and work everywhere.
 */

export const isMac =
  typeof navigator !== "undefined" && /mac/i.test(navigator.platform || navigator.userAgent);

const MODIFIER_KEYS = new Set(["Control", "Shift", "Alt", "Meta"]);

/** Punctuation that requires Shift on a US layout — the shift is already
 *  encoded in the character, so `Shift+/` canonicalizes to `?`, not `shift+?`. */
function normalizeKey(key: string): string {
  if (key === " ") return "space";
  if (key.length === 1) return key.toLowerCase();
  return key.toLowerCase();
}

/** Canonical combo for a keyboard event, or null when only modifiers are down. */
export function keyCombo(e: KeyboardEvent): string | null {
  if (MODIFIER_KEYS.has(e.key)) return null;
  const parts: string[] = [];
  if (isMac ? e.metaKey : e.ctrlKey) parts.push("mod");
  if (e.altKey) parts.push("alt");
  const key = normalizeKey(e.key);
  // Keep `shift` only when it changes the meaning (letters, named keys);
  // shifted punctuation like `?` already carries it.
  if (e.shiftKey && !isShiftedPunctuation(e.key)) parts.push("shift");
  parts.push(key);
  return parts.join("+");
}

function isShiftedPunctuation(key: string): boolean {
  return key.length === 1 && !/[a-z0-9]/i.test(key);
}

const NAMED_KEY_LABELS: Record<string, string> = {
  space: "Space",
  enter: "Enter",
  escape: "Esc",
  backspace: "⌫",
  delete: "Del",
  tab: "Tab",
  arrowup: "↑",
  arrowdown: "↓",
  arrowleft: "←",
  arrowright: "→",
  home: "Home",
  end: "End",
  pageup: "PgUp",
  pagedown: "PgDn",
  ",": ",",
};

/** Human label for a combo, e.g. `mod+k` → `⌘K` (mac) / `Ctrl+K`. */
export function comboLabel(combo: string): string {
  const parts = combo.split("+");
  const out: string[] = [];
  for (const part of parts) {
    if (part === "mod") out.push(isMac ? "⌘" : "Ctrl");
    else if (part === "alt") out.push(isMac ? "⌥" : "Alt");
    else if (part === "shift") out.push(isMac ? "⇧" : "Shift");
    else if (part.length === 1 && /[a-z]/i.test(part)) out.push(part.toUpperCase());
    else out.push(NAMED_KEY_LABELS[part] ?? part);
  }
  return out.join(isMac ? "" : "+");
}
