/**
 * Open the add dialog of Connections directly on one entry of the selection.
 *
 * A screen that knows which connection is missing (the preview of a package
 * before it is installed, for one) asks for it by the entry's identifier: the
 * Connections screen opens, then the add dialog on that entry's warning, with
 * no catalogue to browse first. The identifier is the entry's registry name or
 * the identifier of its package.
 */
import { get, writable } from "svelte/store";
import { navigateTo } from "$lib/stores/navigation";
import type { RegistryServerView } from "$lib/types";

/** The entry the add dialog is asked to open on, until the Connections screen takes it. */
export const pendingConnectionAdd = writable<string | null>(null);

/** Open Connections and its add dialog on the selection entry `entryId`. */
export function requestConnectionAdd(entryId: string): void {
  pendingConnectionAdd.set(entryId);
  navigateTo("integrations");
}

/** Take the pending entry, if any, and clear it so it opens once. */
export function takePendingConnectionAdd(): string | null {
  const entryId = get(pendingConnectionAdd);
  if (entryId !== null) pendingConnectionAdd.set(null);
  return entryId;
}

/** The selection entry named `entryId`, by registry name or package identifier. */
export function findSelectionEntry(
  entries: readonly RegistryServerView[],
  entryId: string,
): RegistryServerView | null {
  return (
    entries.find((e) => e.name === entryId) ??
    entries.find((e) => (e.packages ?? []).some((p) => p.identifier === entryId)) ??
    null
  );
}
