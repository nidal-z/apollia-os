/**
 * Typed Tauri IPC wrappers for the registry of sources.
 *
 * A source is what an agent may consult once it is checked on its sheet.
 * Thin wrappers over the commands of
 * `crates/apollia-desktop/src/commands/sources.rs`, each of which calls one
 * runtime route. Error handling stays at the call site.
 */
import { invoke } from "@tauri-apps/api/core";

/** What a source is. */
export type SourceKind = "folder" | "file" | "mail" | "profile" | "memory" | "code_folder";

/** Where the index of a source stands. */
export type IndexState = "absent" | "indexing" | "ready" | "error" | "keywords_only";

/** A source of the registry, as `GET /api/v1/sources` answers it. */
export interface Source {
  /** The form an agent's `sources` list names it by. */
  id: string;
  kind: SourceKind;
  label: string;
  group: string | null;
  /** A folder connection, a file path or a mailbox. */
  reference: string | null;
  indexState: IndexState;
  /** RFC 3339 UTC. */
  addedAt: string;
}

/** A file left out of an index. */
export interface SkippedFile {
  path: string;
  sizeBytes: number;
  /** Over 50 MB, or no reader could open it. */
  reason: "too_large" | "unreadable";
}

/** Where the index of a source stands, as `GET /api/v1/sources/{id}/index` answers it. */
export interface SourceIndex {
  id: string;
  indexState: IndexState;
  /** Documents the index holds. */
  documents: number;
  chunks: number;
  skipped: SkippedFile[];
}

/** A source to add. */
export interface NewSource {
  kind: SourceKind;
  label: string;
  reference: string | null;
}

/** Every source of the registry, oldest first. */
export function listSources(): Promise<Source[]> {
  return invoke<Source[]>("list_sources");
}

/** Add a source to the registry; answers the source as stored. */
export function addSource(source: NewSource): Promise<Source> {
  return invoke<Source>("add_source", { source });
}

/** Remove the source `sourceId` and its index; answers the source it was. */
export function removeSource(sourceId: string): Promise<Source> {
  return invoke<Source>("remove_source", { sourceId });
}

/** Where the index of the source `sourceId` stands, and the files it left out. */
export function getSourceIndex(sourceId: string): Promise<SourceIndex> {
  return invoke<SourceIndex>("get_source_index", { sourceId });
}

/** The kinds a source made on a folder connection may take. */
export function listSourceFolderKinds(developerMode: boolean): Promise<SourceKind[]> {
  return invoke<SourceKind[]>("list_source_folder_kinds", { developerMode });
}
