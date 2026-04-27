/**
 * Selection store for the inbox bulk actions.
 *
 * Holds the set of selected inbox item ids. Helpers cover toggle, clear,
 * and bulk select/deselect on a provided visible-ids list.
 */
import { writable, derived, get } from "svelte/store";

export const selectedIds = writable<Set<string>>(new Set());

export const selectionCount = derived(selectedIds, ($s) => $s.size);

export function toggle(id: string): void {
  selectedIds.update((s) => {
    const next = new Set(s);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    return next;
  });
}

export function clear(): void {
  selectedIds.set(new Set());
}

export function selectAll(ids: string[]): void {
  selectedIds.set(new Set(ids));
}

export function isSelected(id: string): boolean {
  return get(selectedIds).has(id);
}

/** True iff every visible id is in the selection set. */
export function allVisibleSelected(visible: string[]): boolean {
  if (visible.length === 0) return false;
  const s = get(selectedIds);
  return visible.every((id) => s.has(id));
}
