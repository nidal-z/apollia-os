/**
 * Fuzzy filtering + ranking for the command palette. Kept out of the
 * component so the reactive dialog stays focused on rendering + keyboard.
 */
import { fuzzyMatch } from "$lib/utils/fuzzy";
import type { CommandItem } from "$lib/stores/commandPalette";
import type { CommandPaletteGroup } from "./types";

export interface RankedGroup {
  label: string;
  items: CommandItem[];
}

/** Rank each group by fuzzy score against `query`; drop empty groups. */
export function rankGroups(
  groups: CommandPaletteGroup[],
  query: string,
): RankedGroup[] {
  const q = query.trim();
  const out: RankedGroup[] = [];
  for (const g of groups) {
    const ranked: Array<{ item: CommandItem; score: number }> = [];
    for (const item of g.items) {
      const score = fuzzyMatch(item.label, item.keywords, q);
      if (score !== null) ranked.push({ item, score });
    }
    if (q) ranked.sort((a, b) => b.score - a.score);
    if (ranked.length > 0) {
      out.push({ label: g.label, items: ranked.map((r) => r.item) });
    }
  }
  return out;
}
