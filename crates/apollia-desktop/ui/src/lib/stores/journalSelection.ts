/**
 * The journal entry whose detail the detail sidebar shows.
 *
 * The journal screen writes it when the user asks to see more of an entry, and
 * clears it when it is left; the detail sidebar reads it to show its "Entry"
 * tab. `null` means no entry is open, and the tab is not offered.
 */
import { writable } from "svelte/store";
import type { JournalEntryDetail, JournalLine } from "$lib/ipc/journal";

/** Where the detail of an entry stands. */
export type JournalDetailState =
  | { status: "loading" }
  | { status: "ready"; detail: JournalEntryDetail }
  /** The journal no longer holds the entry. */
  | { status: "missing" }
  | { status: "error" };

export interface JournalSelection {
  line: JournalLine;
  state: JournalDetailState;
}

export const journalSelection = writable<JournalSelection | null>(null);
