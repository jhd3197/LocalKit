/**
 * Tiny fuzzy matcher for the command palette (ported from Faro).
 * Case-insensitive; substring matches score best, then contiguous
 * subsequence matches, then sparse ones. Returns -1 for no match.
 */
export function fuzzyScore(query: string, text: string): number {
  const q = query.trim().toLowerCase();
  if (!q) return 0;
  const t = text.toLowerCase();

  const sub = t.indexOf(q);
  if (sub !== -1) return 1000 - sub * 10 - (t.length - q.length);

  let ti = 0;
  let score = 0;
  let lastMatch = -2;
  for (let qi = 0; qi < q.length; qi++) {
    const found = t.indexOf(q[qi], ti);
    if (found === -1) return -1;
    score += found === lastMatch + 1 ? 10 : 1; // contiguity bonus
    lastMatch = found;
    ti = found + 1;
  }
  return score;
}

/** Filter + rank `items` by fuzzy-matching `query` against `text(item)`. */
export function fuzzyFilter<T>(items: T[], query: string, text: (item: T) => string): T[] {
  if (!query.trim()) return items;
  return items
    .map((item) => ({ item, score: fuzzyScore(query, text(item)) }))
    .filter((x) => x.score >= 0)
    .sort((a, b) => b.score - a.score)
    .map((x) => x.item);
}
