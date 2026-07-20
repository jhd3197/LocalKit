/**
 * Remappable shortcut bindings (plan 15, phase 3).
 *
 * Overrides live in the plan-13 settings store (`app_settings` KV) under
 * `shortcut.<commandId>` keys — no new backend surface beyond a delete:
 *   key absent        → the command's default binding applies
 *   value = "none"    → explicitly unbound (conflict overwrite did this)
 *   value = <combo>   → user override
 * One resolver (`effectiveCombo`) is used by the dispatcher, the palette,
 * the cheat-sheet and Settings → Keyboard — no duplicated maps.
 */

export const SHORTCUT_PREFIX = "shortcut.";
/** Sentinel stored when a binding is explicitly removed (overwrite conflict). */
export const UNBOUND = "none";

export interface Bindable {
  id: string;
  defaultCombo?: string;
}

/** The binding a command fires on right now. */
export function effectiveCombo(
  cmd: Bindable,
  values: Record<string, string>
): string | undefined {
  const override = values[SHORTCUT_PREFIX + cmd.id];
  if (override === UNBOUND) return undefined;
  return override ?? cmd.defaultCombo;
}

/** True when the user has overridden (or unbound) this command's binding. */
export function hasOverride(cmd: Bindable, values: Record<string, string>): boolean {
  return values[SHORTCUT_PREFIX + cmd.id] !== undefined;
}

/** First command (other than `exceptId`) effectively bound to `combo`. */
export function findConflict<T extends Bindable>(
  commands: T[],
  combo: string,
  values: Record<string, string>,
  exceptId: string
): T | undefined {
  return commands.find((c) => c.id !== exceptId && effectiveCombo(c, values) === combo);
}
