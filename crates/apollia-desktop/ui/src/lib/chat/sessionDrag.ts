// Dragging a conversation of the Chat list onto a folder, or out of every
// folder, with the pointer.
//
// The pointer and not the drag and drop of HTML: the window takes the drops
// of the system for itself (a package or a file dropped on the application),
// and while it does the webview delivers no `drop` to the page.

/** The attribute a place that takes a conversation carries: a folder id, or empty for no folder. */
export const DROP_ATTRIBUTE = "data-session-drop";

/** How far the pointer travels, in pixels, before a press becomes a drag. */
export const DRAG_THRESHOLD_PX = 6;

/** A point of the window. */
export interface Point {
  x: number;
  y: number;
}

/** Where a conversation would land: a folder, or out of every folder. */
export interface DropTarget {
  folderId: string | null;
}

/** Whether the pointer moved far enough from `from` for the press to be a drag. */
export function startsDrag(from: Point, to: Point): boolean {
  return Math.hypot(to.x - from.x, to.y - from.y) >= DRAG_THRESHOLD_PX;
}

/**
 * Whether a press on `target` never drags the conversation: a field being
 * typed in, where moving the pointer selects text.
 */
export function refusesDrag(target: EventTarget | null): boolean {
  if (!(target instanceof Element)) return false;
  return target.closest("input, textarea, [contenteditable='true']") !== null;
}

/** The place `element` belongs to, or `null` when it takes no conversation. */
export function dropTargetOf(element: Element | null): DropTarget | null {
  const place = element?.closest(`[${DROP_ATTRIBUTE}]`) ?? null;
  if (!place) return null;
  const folderId = place.getAttribute(DROP_ATTRIBUTE) ?? "";
  return { folderId: folderId === "" ? null : folderId };
}

/**
 * Whether dropping on `target` changes anything for a conversation that sits
 * in `current`: a place that takes it, other than the one it is already in.
 */
export function wouldMove(target: DropTarget | null, current: string | null): boolean {
  return target !== null && target.folderId !== current;
}
