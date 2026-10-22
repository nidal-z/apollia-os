/**
 * The place of the Connections screen, kept while the application runs: the
 * connection whose card is open, the words typed in the search and the kind
 * chip. Leaving the screen for another one and coming back finds them as they
 * were, although the screen itself is mounted again.
 *
 * Another screen that knows a connection opens the page on it with
 * `requestConnection(id)`.
 */
import { writable } from "svelte/store";
import { navigateTo } from "$lib/stores/navigation";
import type { KindFilter, RowSelection } from "$lib/connections/connectionRows";

/** Where the Connections screen stands. */
export interface ConnectionsView {
  selection: RowSelection | null;
  query: string;
  kind: KindFilter;
}

/** The screen at its first opening: no card, no search, every kind. */
export const INITIAL_CONNECTIONS_VIEW: ConnectionsView = { selection: null, query: "", kind: "all" };

const view = writable<ConnectionsView>(INITIAL_CONNECTIONS_VIEW);

/** The place of the Connections screen. */
export const connectionsView = {
  subscribe: view.subscribe,
  /** Open the card of `selection`, or close the card with null. */
  select(selection: RowSelection | null): void {
    view.update((v) => ({ ...v, selection }));
  },
  /** Keep the words typed in the search. */
  search(query: string): void {
    view.update((v) => ({ ...v, query }));
  },
  /** Keep the kind chip chosen. */
  filter(kind: KindFilter): void {
    view.update((v) => ({ ...v, kind }));
  },
  /** Back to the first opening. */
  reset(): void {
    view.set(INITIAL_CONNECTIONS_VIEW);
  },
};

/**
 * Open the Connections screen on the card of the connection `id`. The search
 * and the chip are cleared, so the row of that connection is in the list.
 */
export function requestConnection(id: string): void {
  view.set({ selection: { kind: "connection", id }, query: "", kind: "all" });
  navigateTo("integrations");
}
