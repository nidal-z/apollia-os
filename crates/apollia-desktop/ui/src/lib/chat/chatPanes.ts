/**
 * The panes of the chat: up to four conversations side by side, one of them
 * active.
 *
 * A pane shows one conversation, or none while the user picks one. The same
 * conversation is never shown twice: asking for one that is already on screen
 * activates its pane. The list of conversations, the selector shortcut and
 * the right panel act on the active pane. In a narrow window one pane shows
 * at a time and the others are reached by tabs.
 *
 * The rules are pure functions over a layout; `createChatPanes` holds one in
 * a store and tells its owner which panes stopped showing a conversation, so
 * the model lease of a pane is released when the pane is.
 */
import { writable, type Readable } from "svelte/store";
import { MAIN_PANE } from "$lib/chat/modelLease";

/** How many conversations show side by side at most. */
export const MAX_PANES = 4;

export interface ChatPane {
  /** Stable for the life of the pane; the key of its model lease. */
  id: string;
  /** The conversation shown, `null` while none is chosen. */
  sessionId: string | null;
}

export interface PaneLayout {
  /** In display order; never empty. */
  panes: ChatPane[];
  /** The pane the list, the shortcuts and the right panel act on. */
  activeId: string;
}

/** What a conversation is told of the pane that shows it. */
export interface PaneSeat {
  id: string;
  /** Whether the list, the shortcuts and the right panel act on this pane. */
  active: boolean;
  /** Whether other panes are open beside this one. */
  divided: boolean;
  /** Opens one more pane; absent when the pane cannot be split from. */
  split?: () => void;
  /** Whether one more pane fits. */
  canSplit?: boolean;
  /** Makes this pane the active one. */
  activate?: () => void;
}

/** The seat of a conversation shown alone, outside the panes of the chat. */
export const SOLE_SEAT: PaneSeat = { id: MAIN_PANE, active: true, divided: false };

/** One empty pane: the chat as it opens. */
export function singlePane(): PaneLayout {
  return { panes: [{ id: MAIN_PANE, sessionId: null }], activeId: MAIN_PANE };
}

/** Whether one more pane fits. */
export function canSplit(layout: PaneLayout): boolean {
  return layout.panes.length < MAX_PANES;
}

/** An identifier no pane of `layout` holds. */
function freePaneId(layout: PaneLayout): string {
  const taken = new Set(layout.panes.map((p) => p.id));
  for (let n = 2; ; n++) {
    const id = `pane-${n}`;
    if (!taken.has(id)) return id;
  }
}

/**
 * One more pane, empty and active, right after the active one. Nothing
 * changes once four panes are open.
 */
export function splitPane(layout: PaneLayout): PaneLayout {
  if (!canSplit(layout)) return layout;
  const id = freePaneId(layout);
  const at = layout.panes.findIndex((p) => p.id === layout.activeId);
  const panes = [...layout.panes];
  panes.splice(at + 1, 0, { id, sessionId: null });
  return { panes, activeId: id };
}

/** `id` becomes the active pane, when it exists. */
export function activatePane(layout: PaneLayout, id: string): PaneLayout {
  if (layout.activeId === id || !layout.panes.some((p) => p.id === id)) return layout;
  return { ...layout, activeId: id };
}

/**
 * Shows `sessionId` in the pane `paneId`, the active one by default. A
 * conversation already on screen is not shown twice: its pane is activated.
 */
export function showConversation(
  layout: PaneLayout,
  sessionId: string,
  paneId: string = layout.activeId,
): PaneLayout {
  const shownIn = layout.panes.find((p) => p.sessionId === sessionId);
  if (shownIn) return activatePane(layout, shownIn.id);
  if (!layout.panes.some((p) => p.id === paneId)) return layout;
  return {
    panes: layout.panes.map((p) => (p.id === paneId ? { ...p, sessionId } : p)),
    activeId: paneId,
  };
}

/** The pane `id` shows no conversation any more, and stays open. */
export function emptyPane(layout: PaneLayout, id: string): PaneLayout {
  if (!layout.panes.some((p) => p.id === id && p.sessionId !== null)) return layout;
  return {
    ...layout,
    panes: layout.panes.map((p) => (p.id === id ? { ...p, sessionId: null } : p)),
  };
}

/**
 * Closes the pane `id`. The last pane is never removed: it is emptied. When
 * the active pane closes, its neighbour on the left takes over, else the
 * one on the right.
 */
export function closePane(layout: PaneLayout, id: string): PaneLayout {
  const at = layout.panes.findIndex((p) => p.id === id);
  if (at < 0) return layout;
  if (layout.panes.length === 1) return emptyPane(layout, id);
  const panes = layout.panes.filter((p) => p.id !== id);
  const activeId =
    layout.activeId === id ? panes[Math.max(0, at - 1)]!.id : layout.activeId;
  return { panes, activeId };
}

/** The conversation `sessionId` is gone: the pane that showed it closes. */
export function forgetConversation(layout: PaneLayout, sessionId: string): PaneLayout {
  const shownIn = layout.panes.find((p) => p.sessionId === sessionId);
  return shownIn ? closePane(layout, shownIn.id) : layout;
}

/** The conversation of the active pane, `null` when it shows none. */
export function activeConversation(layout: PaneLayout): string | null {
  return layout.panes.find((p) => p.id === layout.activeId)?.sessionId ?? null;
}

/** Whether several panes are open. */
export function isDivided(layout: PaneLayout): boolean {
  return layout.panes.length > 1;
}

/**
 * Whether the pane `id` is on screen: every pane in a wide window, the
 * active one alone in a narrow window.
 */
export function paneShown(layout: PaneLayout, id: string, narrow: boolean): boolean {
  return narrow ? layout.activeId === id : layout.panes.some((p) => p.id === id);
}

/** Whether the panes are reached by tabs: several of them, in a narrow window. */
export function tabsShown(layout: PaneLayout, narrow: boolean): boolean {
  return narrow && isDivided(layout);
}

/** How the panes are laid out: one, two columns, or two rows of two. */
export interface PaneGrid {
  columns: 1 | 2;
  rows: 1 | 2;
  /** Position of the pane that takes the whole width of its row, if any. */
  wide: number | null;
}

/** The grid of `count` panes; a narrow window stacks nothing, one pane shows. */
export function paneGrid(count: number, narrow: boolean): PaneGrid {
  if (narrow || count <= 1) return { columns: 1, rows: 1, wide: null };
  if (count === 2) return { columns: 2, rows: 1, wide: null };
  return { columns: 2, rows: 2, wide: count === 3 ? 2 : null };
}

/** The panes of `before` that show no conversation in `after`, closed or emptied. */
export function releasedPanes(before: PaneLayout, after: PaneLayout): string[] {
  return before.panes
    .filter((p) => p.sessionId !== null)
    .filter((p) => (after.panes.find((q) => q.id === p.id)?.sessionId ?? null) === null)
    .map((p) => p.id);
}

export interface ChatPanes extends Readable<PaneLayout> {
  /** Opens one more pane; `false` when four are open already. */
  split: () => boolean;
  activate: (id: string) => void;
  /** Shows a conversation in a pane, the active one by default. */
  show: (sessionId: string, paneId?: string) => void;
  /** Empties a pane without closing it. */
  empty: (id: string) => void;
  close: (id: string) => void;
  /** A conversation was removed: its pane closes. */
  forget: (sessionId: string) => void;
}

/**
 * The panes of one chat. `release` is called with each pane that stopped
 * showing a conversation, once the layout has changed.
 */
export function createChatPanes(release: (paneId: string) => void): ChatPanes {
  let current = singlePane();
  const store = writable<PaneLayout>(current);

  function apply(next: PaneLayout): void {
    if (next === current) return;
    const released = releasedPanes(current, next);
    current = next;
    store.set(next);
    for (const id of released) release(id);
  }

  return {
    subscribe: store.subscribe,
    split() {
      const before = current;
      apply(splitPane(before));
      return current !== before;
    },
    activate: (id) => apply(activatePane(current, id)),
    show: (sessionId, paneId) => apply(showConversation(current, sessionId, paneId)),
    empty: (id) => apply(emptyPane(current, id)),
    close: (id) => apply(closePane(current, id)),
    forget: (sessionId) => apply(forgetConversation(current, sessionId)),
  };
}
