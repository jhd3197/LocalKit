import { describe, expect, it } from "vitest";
import {
  SHORTCUT_PREFIX,
  UNBOUND,
  effectiveCombo,
  findConflict,
  hasOverride,
} from "./keybindings";

const cmd = (id: string, defaultCombo?: string) => ({ id, defaultCombo });
const override = (id: string, combo: string) => ({ [SHORTCUT_PREFIX + id]: combo });

describe("effectiveCombo", () => {
  it("falls back to the default when there's no override", () => {
    expect(effectiveCombo(cmd("palette", "mod+k"), {})).toBe("mod+k");
  });

  it("uses an override over the default", () => {
    expect(effectiveCombo(cmd("palette", "mod+k"), override("palette", "mod+p"))).toBe("mod+p");
  });

  it("treats the UNBOUND sentinel as no binding", () => {
    expect(effectiveCombo(cmd("palette", "mod+k"), override("palette", UNBOUND))).toBeUndefined();
  });
});

describe("hasOverride", () => {
  it("is true only when a key is present (UNBOUND counts)", () => {
    expect(hasOverride(cmd("x", "a"), {})).toBe(false);
    expect(hasOverride(cmd("x", "a"), override("x", UNBOUND))).toBe(true);
    expect(hasOverride(cmd("x", "a"), override("x", "b"))).toBe(true);
  });
});

describe("findConflict", () => {
  const cmds = [cmd("a", "mod+k"), cmd("b", "mod+p"), cmd("c")];

  it("finds another command effectively bound to the combo", () => {
    expect(findConflict(cmds, "mod+k", {}, "b")?.id).toBe("a");
  });

  it("never reports the command against itself", () => {
    expect(findConflict(cmds, "mod+k", {}, "a")).toBeUndefined();
  });

  it("resolves through overrides", () => {
    // c is rebound onto mod+k; searching (except a) surfaces c, not a.
    expect(findConflict(cmds, "mod+k", override("c", "mod+k"), "a")?.id).toBe("c");
  });
});
