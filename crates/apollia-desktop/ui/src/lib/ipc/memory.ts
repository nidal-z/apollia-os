/**
 * Typed Tauri IPC wrappers for the memory explorer and for namespace-level
 * data ownership (export, import, purge).
 *
 * Keeping the `invoke` calls here removes direct Tauri usage from the memory
 * `.svelte` components. Entry listing and agent listing have their wrappers
 * in `$lib/ipc/agents` and `$lib/ipc/connections`; this module covers the
 * namespaces and the memory-specific commands.
 *
 * Argument names are camelCase on purpose: Tauri maps them to the snake_case
 * parameters of `crates/apollia-desktop/src/commands/memory.rs`. A misspelled
 * key is not a compile error, it is a runtime rejection.
 */
import { invoke } from "@tauri-apps/api/core";
import type { MemorySearchResult, InjectedEntry, MemoryEmbeddings } from "$lib/types";

/** Every memory namespace, for the explorer. */
export function listMemoryNamespaces(): Promise<string[]> {
  return invoke<string[]>("list_memory_namespaces");
}

/** Full-text search within a namespace. */
export function searchMemory(
  namespace: string,
  query: string,
): Promise<MemorySearchResult[]> {
  return invoke<MemorySearchResult[]>("search_memory", { namespace, query });
}

/**
 * The embeddings model the memory searches with; both fields are null when
 * the memory is searched by its words alone.
 */
export function getMemoryEmbeddingsModel(): Promise<MemoryEmbeddings> {
  return invoke<MemoryEmbeddings>("get_memory_embeddings_model");
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
