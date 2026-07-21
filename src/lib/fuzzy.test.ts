import { describe, expect, it } from "vitest";
import { fuzzyFilter, fuzzyScore } from "./fuzzy";

describe("fuzzyScore", () => {
  it("returns 0 for an empty query (everything matches)", () => {
    expect(fuzzyScore("", "anything")).toBe(0);
    expect(fuzzyScore("   ", "anything")).toBe(0);
  });

  it("is case-insensitive", () => {
    expect(fuzzyScore("BLOG", "my blog")).toBeGreaterThan(0);
  });

  it("scores a substring above a sparse subsequence", () => {
    const sub = fuzzyScore("blog", "My Blog");
    const sparse = fuzzyScore("mg", "My Blog");
    expect(sub).toBeGreaterThan(sparse);
    expect(sparse).toBeGreaterThan(0);
  });

  it("prefers an earlier substring position", () => {
    expect(fuzzyScore("a", "abc")).toBeGreaterThan(fuzzyScore("a", "xxa"));
  });

  it("returns -1 when a character can't be found", () => {
    expect(fuzzyScore("xyz", "my blog")).toBe(-1);
  });
});

describe("fuzzyFilter", () => {
  const items = ["Open site", "Start site", "Stop site", "Delete site"];

  it("returns the input untouched for a blank query", () => {
    expect(fuzzyFilter(items, "  ", (s) => s)).toBe(items);
  });

  it("filters out non-matches and ranks best-first", () => {
    const out = fuzzyFilter(items, "sta", (s) => s);
    expect(out[0]).toBe("Start site");
    expect(out).not.toContain("Delete site");
  });
});
