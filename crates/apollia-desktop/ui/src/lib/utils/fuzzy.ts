/**
 * Tiny fuzzy subsequence scorer (US-SP42-017).
 *
 * Returns a score in [0, 1] for how well `query` matches `text`, or
 * `null` if the query characters do not appear as a subsequence of
 * `text`. Case-insensitive. Higher scores favour:
 *  - contiguous matches,
 *  - matches at the start of the string,
 *  - matches at word boundaries (after space / `-` / `_` / `.`).
 */
export function fuzzyScore(text: string, query: string): number | null {
  if (!query) return 0.0001;
  const t = text.toLowerCase();
  const q = query.toLowerCase();

  let ti = 0;
  let qi = 0;
  let score = 0;
  let streak = 0;
  let prevMatch = -2;

  while (ti < t.length && qi < q.length) {
    if (t[ti] === q[qi]) {
      // Bonus: start of string.
      if (ti === 0) score += 0.2;
      // Bonus: word boundary.
      else if (/[\s\-_.]/.test(t[ti - 1])) score += 0.15;
      // Bonus: contiguous with previous match.
      if (prevMatch === ti - 1) {
        streak += 1;
        score += 0.1 + streak * 0.05;
      } else {
        streak = 0;
      }
      score += 0.05;
      prevMatch = ti;
      qi += 1;
    }
    ti += 1;
  }

  if (qi < q.length) return null;

  // Normalise lightly by text length so shorter targets win ties.
  const normalised = score / (1 + Math.log1p(t.length) * 0.1);
  return normalised;
}

/**
 * Fuzzy match across `label + keywords` (keywords get a small bonus).
 * Returns `null` if neither label nor any keyword matches.
 */
export function fuzzyMatch(
  label: string,
  keywords: string[] | undefined,
  query: string,
): number | null {
  const labelScore = fuzzyScore(label, query);
  let best = labelScore;
  if (keywords) {
    for (const kw of keywords) {
      const s = fuzzyScore(kw, query);
      if (s !== null) {
        const boosted = s * 0.85;
        if (best === null || boosted > best) best = boosted;
      }
    }
  }
  return best;
}
