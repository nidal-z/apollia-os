/**
 * j/k message navigation helper.
 *
 * Used by `Chat.svelte` to step focus across rendered chat messages.
 * Selectors are resolved lazily — the active conversation may not be
 * mounted when the shortcut binding is created.
 */

const MESSAGE_SELECTOR = "[data-message-id]";

export function focusMessage(direction: 1 | -1): void {
  const root = document.querySelector<HTMLElement>(
    "[data-testid='chat-messages-list']",
  );
  if (!root) return;
  const items = Array.from(root.querySelectorAll<HTMLElement>(MESSAGE_SELECTOR));
  if (items.length === 0) return;

  const active = document.activeElement as HTMLElement | null;
  let idx = active ? items.findIndex((el) => el === active || el.contains(active)) : -1;
  if (idx === -1) {
    idx = direction === 1 ? 0 : items.length - 1;
  } else {
    idx = Math.max(0, Math.min(items.length - 1, idx + direction));
  }
  const target = items[idx];
  // Bubbles need an explicit `tabindex` to receive focus — fall back if
  // the renderer hasn't tagged them yet.
  if (!target.hasAttribute("tabindex")) target.setAttribute("tabindex", "-1");
  target.focus({ preventScroll: false });
  target.scrollIntoView({ block: "nearest" });
}
