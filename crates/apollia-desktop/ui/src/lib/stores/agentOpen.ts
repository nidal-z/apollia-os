/**
 * Open the Agents screen directly on the sheet of one agent or workflow.
 *
 * A screen that lists an agent or a workflow (the content of a package in the
 * Library, for one) asks for its sheet by identifier: the Agents screen opens,
 * takes the request, and shows the sheet.
 */
import { get, writable } from "svelte/store";
import { navigateTo } from "$lib/stores/navigation";

/** The sheet asked for: an agent by its id, a workflow as `<package>/<workflow>`. */
export interface SheetRequest {
  kind: "agent" | "workflow";
  id: string;
}

/** The sheet the Agents screen is asked to open, until it takes it. */
export const pendingSheet = writable<SheetRequest | null>(null);

/** Open the Agents screen on the sheet `request` names. */
export function requestSheet(request: SheetRequest): void {
  pendingSheet.set(request);
  navigateTo("agents");
}

/** Take the pending request, if any, and clear it so it opens once. */
export function takePendingSheet(): SheetRequest | null {
  const request = get(pendingSheet);
  if (request !== null) pendingSheet.set(null);
  return request;
}
