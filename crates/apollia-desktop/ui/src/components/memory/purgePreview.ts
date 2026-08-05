/**
 * Purge preview arithmetic, split out of `NamespaceDataActions.svelte` so the
 * "how many entries am I about to lose" answer is testable on its own.
 *
 * `purge_memory` offers no dry run, so this counts the entries the explorer
 * already listed. It is a lower bound, never a promise: the runtime return
 * value is the authoritative figure.
 */
import type { MemoryEntry } from "$lib/types";
import type { MemoryPurgeScope } from "$lib/ipc/memory";

const MS_PER_DAY = 86_400_000;

/**
 * Number of `entries` a purge of `scope` older than `days` days would take.
 *
 * `nowMs` is injected so the count is deterministic under test. Entries with an
 * unparsable `created_at` are excluded: an entry whose age cannot be
 * established must never be counted as doomed.
 */
export function countPurgeMatches(
  entries: MemoryEntry[],
  scope: MemoryPurgeScope,
  days: number,
  nowMs: number,
): number {
  const cutoff = nowMs - days * MS_PER_DAY;
  return entries.filter((entry) => {
    if (scope !== "all" && entry.entry_type !== scope) return false;
    const created = Date.parse(entry.created_at);
    return Number.isFinite(created) && created < cutoff;
  }).length;
}
