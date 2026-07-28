/**
 * listNavigation - roving-tabindex keyboard navigation for a `ListPanel`.
 *
 * A Svelte action applied to the list container. It gives Up / Down / Home /
 * End movement across the `ListRow`s inside, using the roving-tabindex pattern
 * (one row is tabbable at a time) and mirrors focus onto the container via
 * `aria-activedescendant`. Row activation (Enter / Space) already lives in
 * `ListRow`, so this action never handles it.
 *
 * Usage:
 *
 *   <div use:listNavigation>
 *     <ListPanel>
 *       {#each items as item (item.id)}
 *         <ListRow onclick={...}>...</ListRow>
 *       {/each}
 *     </ListPanel>
 *   </div>
 *
 * Rows are discovered by `rowSelector` (default `[role="button"]`, which
 * `ListRow` sets when given an `onclick`). Reduced motion does not apply here.
 */
import type { Action } from "svelte/action";

export interface ListNavigationOptions {
  /** CSS selector for the navigable rows. */
  rowSelector?: string;
  /** Prefix for generated row ids used by `aria-activedescendant`. */
  idPrefix?: string;
}

const NAV_KEYS = new Set(["ArrowDown", "ArrowUp", "Home", "End"]);

export const listNavigation: Action<HTMLElement, ListNavigationOptions | undefined> = (
  node,
  options,
) => {
  let rowSelector = options?.rowSelector ?? '[role="button"]';
  let idPrefix = options?.idPrefix ?? "list-nav-item";
  let activeIndex = 0;
  let seq = 0;

  function rows(): HTMLElement[] {
    return Array.from(node.querySelectorAll<HTMLElement>(rowSelector)).filter(
      (el) => !el.hasAttribute("disabled") && el.getAttribute("aria-disabled") !== "true",
    );
  }

  function ensureId(el: HTMLElement): string {
    if (!el.id) el.id = `${idPrefix}-${seq++}`;
    return el.id;
  }

  function applyTabindex(): void {
    const list = rows();
    if (list.length === 0) return;
    activeIndex = Math.max(0, Math.min(activeIndex, list.length - 1));
    list.forEach((el, i) => {
      el.tabIndex = i === activeIndex ? 0 : -1;
    });
  }

  function focusIndex(index: number): void {
    const list = rows();
    if (list.length === 0) return;
    activeIndex = Math.max(0, Math.min(index, list.length - 1));
    list.forEach((el, i) => {
      el.tabIndex = i === activeIndex ? 0 : -1;
    });
    const target = list[activeIndex];
    const id = ensureId(target);
    node.setAttribute("aria-activedescendant", id);
    target.focus();
  }

  function onKeydown(event: KeyboardEvent): void {
    if (!NAV_KEYS.has(event.key)) return;
    const list = rows();
    if (list.length === 0) return;
    event.preventDefault();
    switch (event.key) {
      case "ArrowDown":
        focusIndex(activeIndex + 1);
        break;
      case "ArrowUp":
        focusIndex(activeIndex - 1);
        break;
      case "Home":
        focusIndex(0);
        break;
      case "End":
        focusIndex(list.length - 1);
        break;
    }
  }

  function onFocusIn(event: FocusEvent): void {
    const list = rows();
    const idx = list.indexOf(event.target as HTMLElement);
    if (idx >= 0) {
      activeIndex = idx;
      node.setAttribute("aria-activedescendant", ensureId(list[idx]));
    }
  }

  node.addEventListener("keydown", onKeydown);
  node.addEventListener("focusin", onFocusIn);
  applyTabindex();

  return {
    update(next?: ListNavigationOptions) {
      rowSelector = next?.rowSelector ?? '[role="button"]';
      idPrefix = next?.idPrefix ?? "list-nav-item";
      applyTabindex();
    },
    destroy() {
      node.removeEventListener("keydown", onKeydown);
      node.removeEventListener("focusin", onFocusIn);
    },
  };
};
