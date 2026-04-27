/**
 * Modal stack tracker for nested inbox dialogs.
 *
 * When multiple approval dialogs are open at once, we need to (a) render a
 * "Dialog N of M" hint and (b) compute an incremental z-index so deeper
 * dialogs stack on top. bits-ui Dialog keeps its own focus trap; this store
 * only tracks depth.
 */
import { writable, derived, get } from "svelte/store";

const BASE_Z = 1000;

export const modalStack = writable<string[]>([]);

export const stackDepth = derived(modalStack, ($s) => $s.length);

export function push(id: string): number {
  modalStack.update((s) => [...s, id]);
  return BASE_Z + get(modalStack).length;
}

export function pop(id: string): void {
  modalStack.update((s) => s.filter((x) => x !== id));
}

export function positionOf(id: string): number {
  return get(modalStack).indexOf(id) + 1;
}
