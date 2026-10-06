/**
 * How the split chat scrolls sideways: the row of tabs of a narrow window,
 * and the pane area of a wide one when the panes do not fit.
 *
 * Both always scroll, by the wheel, by a drag and by the arrow keys. The
 * scroll bar itself is a display preference of Settings > Appearance: hidden
 * by default, drawn as soon as the content overflows when the reader asks
 * for it. A rail that holds its content draws none either way.
 *
 * The rules are pure functions; `tabRail` wires them to the row of tabs.
 */
import type { PaneGrid } from "$lib/chat/chatPanes";

/** A pointer that moved less than this many pixels pressed, it did not drag. */
export const DRAG_THRESHOLD_PX = 4;

/**
 * The classes of a rail that scrolls sideways. `overflow-x-auto` in both
 * cases, so the rail scrolls whatever the preference and draws a bar only
 * when its content overflows; `scrollbar-none` removes the bar and nothing
 * else.
 */
export function scrollRailClass(showBar: boolean): string {
  return showBar ? "overflow-x-auto" : "overflow-x-auto scrollbar-none";
}

/**
 * The column class of the pane area. Side by side, each pane keeps the
 * minimal width of the design scale, so the area overflows, and scrolls,
 * rather than squeezing a conversation.
 */
export function paneColumnsClass(grid: PaneGrid): string {
  return grid.columns === 2 ? "grid-cols-panes-2" : "grid-cols-1";
}

/**
 * The sideways distance of one wheel event. A wheel that only turns
 * vertically moves the rail sideways; a trackpad keeps its own axis.
 */
export function wheelDistance(deltaX: number, deltaY: number): number {
  return Math.abs(deltaX) >= Math.abs(deltaY) ? deltaX : deltaY;
}

/** Whether a pointer that moved `distance` pixels while pressed is a drag. */
export function isDrag(distance: number): boolean {
  return Math.abs(distance) > DRAG_THRESHOLD_PX;
}

/** The scroll position of a rail dragged from `startScroll` by `distance`. */
export function dragScroll(startScroll: number, distance: number): number {
  return Math.max(0, startScroll - distance);
}

export interface RailView {
  /** Offset of the item from the start of the rail content. */
  itemStart: number;
  itemSize: number;
  /** Current scroll position of the rail. */
  scrollStart: number;
  /** Visible length of the rail. */
  viewSize: number;
}

/**
 * The scroll position that brings an item into the visible part of its
 * rail: unchanged when the item is whole on screen, else the smallest move
 * that shows it. An item longer than the rail is aligned on its start.
 */
export function revealScroll({ itemStart, itemSize, scrollStart, viewSize }: RailView): number {
  if (itemStart < scrollStart || itemSize > viewSize) return Math.max(0, itemStart);
  const itemEnd = itemStart + itemSize;
  if (itemEnd > scrollStart + viewSize) return itemEnd - viewSize;
  return scrollStart;
}

/**
 * The tab an arrow key moves to, `null` when the key is not one of them.
 * The row wraps at both ends; Home and End reach the first and the last.
 */
export function arrowTab(ids: readonly string[], current: string, key: string): string | null {
  if (ids.length === 0) return null;
  const at = Math.max(0, ids.indexOf(current));
  const n = ids.length;
  switch (key) {
    case "ArrowRight":
      return ids[(at + 1) % n] ?? null;
    case "ArrowLeft":
      return ids[(at - 1 + n) % n] ?? null;
    case "Home":
      return ids[0] ?? null;
    case "End":
      return ids[n - 1] ?? null;
    default:
      return null;
  }
}

/** Brings `item` into the visible part of `rail`. */
export function revealIn(rail: HTMLElement, item: HTMLElement): void {
  rail.scrollLeft = revealScroll({
    itemStart: item.offsetLeft - rail.offsetLeft,
    itemSize: item.offsetWidth,
    scrollStart: rail.scrollLeft,
    viewSize: rail.clientWidth,
  });
}

/**
 * Makes a row scroll sideways by the wheel and by a drag. A drag is not a
 * click: the tab under the pointer is not activated when the pointer is
 * released.
 */
export function tabRail(node: HTMLElement): { destroy: () => void } {
  let startX = 0;
  let startScroll = 0;
  let pressed = false;
  let dragged = false;

  function onWheel(e: WheelEvent): void {
    if (node.scrollWidth <= node.clientWidth) return;
    e.preventDefault();
    node.scrollLeft += wheelDistance(e.deltaX, e.deltaY);
  }
  function onPointerDown(e: PointerEvent): void {
    if (e.button !== 0) return;
    pressed = true;
    dragged = false;
    startX = e.clientX;
    startScroll = node.scrollLeft;
  }
  function onPointerMove(e: PointerEvent): void {
    if (!pressed) return;
    const distance = e.clientX - startX;
    if (!dragged && !isDrag(distance)) return;
    dragged = true;
    node.scrollLeft = dragScroll(startScroll, distance);
  }
  function onPointerUp(): void {
    pressed = false;
  }
  function onClick(e: MouseEvent): void {
    if (!dragged) return;
    dragged = false;
    e.preventDefault();
    e.stopPropagation();
  }

  node.addEventListener("wheel", onWheel, { passive: false });
  node.addEventListener("pointerdown", onPointerDown);
  node.addEventListener("pointermove", onPointerMove);
  globalThis.addEventListener("pointerup", onPointerUp);
  node.addEventListener("click", onClick, true);
  return {
    destroy() {
      node.removeEventListener("wheel", onWheel);
      node.removeEventListener("pointerdown", onPointerDown);
      node.removeEventListener("pointermove", onPointerMove);
      globalThis.removeEventListener("pointerup", onPointerUp);
      node.removeEventListener("click", onClick, true);
    },
  };
}
