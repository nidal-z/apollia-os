/**
 * The connection another screen asks the journal to open on. The asking screen
 * writes the prefix the tools of the connection share and the name it shows,
 * then navigates; the journal reads it once on arrival, clears it, and opens
 * filtered on that connection.
 */
import { get, writable } from "svelte/store";

/** A connection the journal is narrowed to. */
export interface JournalConnectionFocus {
  /** The prefix the tools of the connection share, such as `mcp:notion/`. */
  prefix: string;
  /** The name of the connection, as the screens show it. */
  label: string;
}

const requested = writable<JournalConnectionFocus | null>(null);

/** Asks the journal to open filtered on the connection `focus` names. */
export function requestJournalConnection(focus: JournalConnectionFocus): void {
  requested.set({ ...focus });
}

/** The connection asked for, once: `null` when nothing was asked. */
export function takeRequestedJournalConnection(): JournalConnectionFocus | null {
  const focus = get(requested);
  if (focus !== null) requested.set(null);
  return focus;
}
