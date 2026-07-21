import { describe, expect, it } from "vitest";
import { comboLabel, isMac, keyCombo } from "./shortcuts";

// The label + `mod` mapping is platform-dependent; assert against the module's
// own `isMac` so the test is correct on either OS (jsdom reports non-mac).
const mod = isMac ? "⌘" : "Ctrl";
const join = (parts: string[]) => parts.join(isMac ? "" : "+");

describe("comboLabel", () => {
  it("maps mod to the platform modifier", () => {
    expect(comboLabel("mod+k")).toBe(join([mod, "K"]));
  });

  it("labels named keys and stacks modifiers", () => {
    expect(comboLabel("mod+shift+arrowup")).toBe(
      join([mod, isMac ? "⇧" : "Shift", "↑"])
    );
    expect(comboLabel("alt+enter")).toBe(join([isMac ? "⌥" : "Alt", "Enter"]));
  });

  it("leaves shifted punctuation as its own character", () => {
    expect(comboLabel("?")).toBe("?");
  });
});

describe("keyCombo", () => {
  // Set both ctrl+meta so `mod` resolves on either platform.
  const ev = (init: KeyboardEventInit) => new KeyboardEvent("keydown", init);

  it("returns null when only a modifier key is down", () => {
    expect(keyCombo(ev({ key: "Control", ctrlKey: true }))).toBeNull();
    expect(keyCombo(ev({ key: "Shift", shiftKey: true }))).toBeNull();
  });

  it("canonicalizes mod+K", () => {
    expect(keyCombo(ev({ key: "k", ctrlKey: true, metaKey: true }))).toBe("mod+k");
  });

  it("keeps shift for letters but folds it into shifted punctuation", () => {
    expect(keyCombo(ev({ key: "K", ctrlKey: true, metaKey: true, shiftKey: true }))).toBe(
      "mod+shift+k"
    );
    // "?" already encodes its shift — it must not become `shift+?`.
    expect(keyCombo(ev({ key: "?", shiftKey: true }))).toBe("?");
  });

  it("names the space key", () => {
    expect(keyCombo(ev({ key: " " }))).toBe("space");
  });
});
