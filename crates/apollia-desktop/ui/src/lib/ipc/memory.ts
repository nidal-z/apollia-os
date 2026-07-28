/**
 * Typed Tauri IPC wrappers for the memory explorer and insight review.
 *
 * Keeping the `invoke` calls here removes direct Tauri usage from the memory
 * `.svelte` components. Namespace/entry listing and agent listing already have
 * wrappers (`$lib/ipc/projects`, `$lib/ipc/connections`); this module covers
 * the memory-specific commands that had no wrapper yet.
 */
import { invoke } from "@tauri-apps/api/core";
import type { MemorySearchResult, InjectedEntry } from "$lib/types";

/** Full-text search within a namespace. */
export function searchMemory(
  namespace: string,
  query: string,
): Promise<MemorySearchResult[]> {
  return invoke<MemorySearchResult[]>("search_memory", { namespace, query });
}

/** Delete a single memory entry. Resolves `true` when a row was removed. */
export function deleteMemoryEntry(
  namespace: string,
  entryId: string,
): Promise<boolean> {
  return invoke<boolean>("delete_memory_entry", { namespace, entryId });
}

/** Promote an extracted insight into durable memory. */
export function acceptExtractedInsight(id: string): Promise<void> {
  return invoke<void>("accept_extracted_insight", { id });
}

/** Discard an extracted insight, recording the operator's reason. */
export function rejectExtractedInsight(
  id: string,
  reason: string,
): Promise<void> {
  return invoke<void>("reject_extracted_insight", { id, reason });
}

/** Edit an extracted insight before accepting it. */
export function updateExtractedInsight(
  id: string,
  text: string,
  category: string,
): Promise<void> {
  return invoke<void>("update_extracted_insight", { id, text, category });
}

/** Entries that were injected into a given conversation turn. */
export function getInjectedMemoryEntries(
  turnId: string,
): Promise<InjectedEntry[]> {
  return invoke<InjectedEntry[]>("get_injected_memory_entries", { turnId });
}
