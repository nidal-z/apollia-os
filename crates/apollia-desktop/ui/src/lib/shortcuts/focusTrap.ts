/**
 * Focus trap action.
 *
 * Apply with `use:focusTrap` on the root of any modal-like surface.
 * Cycles Tab/Shift+Tab between the first and last tabbable descendants
 * and restores focus to the previously-focused element on teardown.
 *
 * The action is intentionally minimal: it does NOT close the surface
 * on Escape — owners keep that responsibility because dismissal often
 * requires custom logic (toast, "always reject", confirm input, …).
 */

const TABBABLE = [
  "a[href]",
  "button:not([disabled])",
  "input:not([disabled]):not([type='hidden'])",
  "select:not([disabled])",
  "textarea:not([disabled])",
  "[tabindex]:not([tabindex='-1'])",
  "audio[controls]",
  "video[controls]",
  "[contenteditable]:not([contenteditable='false'])",
].join(",");

export interface FocusTrapOptions {
  /** Disable the trap without unmounting (e.g. keep node mounted while dialog hidden). */
  enabled?: boolean;
  /**
   * Optional initial focus target query (within the trap root). When
   * omitted, the first tabbable descendant receives focus on activation.
   */
  initialFocus?: string;
}

function visibleTabbables(root: HTMLElement): HTMLElement[] {
  return Array.from(root.querySelectorAll<HTMLElement>(TABBABLE)).filter(
    (el) => !el.hasAttribute("inert") && el.offsetParent !== null,
  );
}

export function focusTrap(node: HTMLElement, options: FocusTrapOptions = {}) {
  let opts: FocusTrapOptions = { enabled: true, ...options };
  let previouslyFocused = document.activeElement as HTMLElement | null;

  function focusInitial(): void {
    if (!opts.enabled) return;
    if (opts.initialFocus) {
      const target = node.querySelector<HTMLElement>(opts.initialFocus);
      if (target) {
        target.focus();
        return;
      }
    }
    const tabbables = visibleTabbables(node);
    if (tabbables.length > 0) tabbables[0].focus();
    else node.focus();
  }

  function handleKeydown(event: KeyboardEvent): void {
    if (!opts.enabled) return;
    if (event.key !== "Tab") return;
    const tabbables = visibleTabbables(node);
    if (tabbables.length === 0) {
      event.preventDefault();
      node.focus();
      return;
    }
    const first = tabbables[0];
    const last = tabbables[tabbables.length - 1];
    const active = document.activeElement as HTMLElement | null;
    if (event.shiftKey) {
      if (active === first || !node.contains(active)) {
        event.preventDefault();
        last.focus();
      }
    } else if (active === last) {
      event.preventDefault();
      first.focus();
    }
  }

  // Defer initial focus by a microtask so callers can finish layout
  // (autofocus inputs, lazy children, …) before we pick a target.
  queueMicrotask(focusInitial);
  node.addEventListener("keydown", handleKeydown);

  return {
    update(next: FocusTrapOptions) {
      opts = { enabled: true, ...next };
      if (opts.enabled && !node.contains(document.activeElement)) {
        focusInitial();
      }
    },
    destroy() {
      node.removeEventListener("keydown", handleKeydown);
      // Restore previously-focused element only when it is still attached
      // to the document — stale references would noop and steal focus.
      if (previouslyFocused?.isConnected) previouslyFocused.focus();
      previouslyFocused = null;
    },
  };
}
