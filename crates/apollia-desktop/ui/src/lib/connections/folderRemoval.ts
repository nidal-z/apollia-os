/**
 * What removing a folder connection takes with it.
 *
 * Sources may be made on a folder connection: a folder whose documents are
 * read, a code folder. The runtime removes none of them without a
 * confirmation, so the page of the connection lists them first and asks.
 */
import type { IndexState, Source } from "$lib/ipc/sources";

/** A source that goes with the folder connection. */
export interface DependentSource {
  id: string;
  label: string;
  kind: "folder" | "code_folder";
  indexState: IndexState;
}

/** The stored identifier of a folder connection, without its `folder:` prefix. */
function folderId(reference: string): string {
  return reference.startsWith("folder:") ? reference.slice("folder:".length) : reference;
}

/** The sources of `sources` made on the folder connection `connectionId`. */
export function dependentSources(sources: Source[], connectionId: string): DependentSource[] {
  const id = folderId(connectionId);
  const dependents: DependentSource[] = [];
  for (const source of sources) {
    if (source.kind !== "folder" && source.kind !== "code_folder") continue;
    if (source.reference === null || folderId(source.reference) !== id) continue;
    dependents.push({ id: source.id, label: source.label, kind: source.kind, indexState: source.indexState });
  }
  return dependents;
}
