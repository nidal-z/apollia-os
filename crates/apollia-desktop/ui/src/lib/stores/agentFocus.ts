/**
 * The agent another screen asks the Agents screen to open. The asking screen
 * writes the agent's definition identifier and navigates; the Agents screen
 * reads it once on arrival and clears it.
 */
import { get, writable } from "svelte/store";

const requested = writable<string | null>(null);

/** Asks the Agents screen to open the sheet of `definitionId` on arrival. */
export function requestAgentSheet(definitionId: string): void {
  requested.set(definitionId);
}

/** The agent asked for, once: `null` when nothing was asked. */
export function takeRequestedAgent(): string | null {
  const id = get(requested);
  if (id !== null) requested.set(null);
  return id;
}
