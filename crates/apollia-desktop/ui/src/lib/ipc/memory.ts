/**
 * Typed Tauri IPC wrappers for the memory explorer and for namespace-level
 * data ownership (export, import, purge).
 *
 * Keeping the `invoke` calls here removes direct Tauri usage from the memory
 * `.svelte` components. Namespace/entry listing and agent listing already have
 * wrappers (`$lib/ipc/projects`, `$lib/ipc/connections`); this module covers
 * the memory-specific commands that had no wrapper yet.
 *
 * Argument names are camelCase on purpose: Tauri maps them to the snake_case
 * parameters of `crates/apollia-desktop/src/commands/memory.rs`. A misspelled
 * key is not a compile error, it is a runtime rejection.
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

/** Entries that were injected into a given conversation turn. */
export function getInjectedMemoryEntries(
  turnId: string,
): Promise<InjectedEntry[]> {
  return invoke<InjectedEntry[]>("get_injected_memory_entries", { turnId });
}

/**
 * How an import treats what is already in the namespace.
 * - `merge`   inserts the entries whose id is missing, leaves the rest alone.
 * - `replace` wipes the namespace first, then inserts everything.
 */
export type MemoryImportMode = "merge" | "replace";

/** Memory families a purge can target. */
export type MemoryPurgeScope = "all" | "episodic" | "semantic" | "procedural";

/**
 * Write a namespace to a JSON file at `outputPath`.
 *
 * Resolves with a human-readable summary of what was written (per-family
 * counts and the destination path), produced by the runtime.
 */
export function memoryExportNamespace(
  namespace: string,
  outputPath: string,
): Promise<string> {
  return invoke<string>("memory_export_namespace", { namespace, outputPath });
}

/**
 * Read a JSON export from `inputPath` into `namespace`.
 *
 * Resolves with a human-readable summary stating how many entries were
 * imported and under which mode.
 */
export function memoryImportNamespace(
  namespace: string,
  inputPath: string,
  mode: MemoryImportMode,
): Promise<string> {
  return invoke<string>("memory_import_namespace", {
    namespace,
    inputPath,
    mode,
  });
}

/**
 * Delete every entry of `memoryType` created more than `olderThanDays` days
 * ago. Irreversible. Resolves with the number of entries actually removed.
 */
export function purgeMemory(
  namespace: string,
  olderThanDays: number,
  memoryType: MemoryPurgeScope,
): Promise<number> {
  return invoke<number>("purge_memory", {
    namespace,
    olderThanDays,
    memoryType,
  });
}
