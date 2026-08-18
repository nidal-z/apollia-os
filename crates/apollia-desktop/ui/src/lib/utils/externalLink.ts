// External URL helpers for the Tauri webview.
//
// Tauri 2 disables `window.open()` and ignores `<a target="_blank">` by
// default - outbound clicks were silently dead before this module landed.
// The canonical fix is the `tauri-plugin-opener` plugin (paired with the
// `opener:default` + `opener:allow-open-url` capability permissions).
//
// We export two helpers:
//   - `openExternalUrl(url)` - async, returns Promise<void>. Use from event
//     handlers that can `await`.
//   - `handleExternalLinkClick(event)` - synchronous Svelte event handler
//     that intercepts a left-click on a normal `<a href>` link and routes
//     the URL through the opener. Bind with `onclick={handleExternalLinkClick}`.
//
// Both fall back to `window.open` in the browser dev preview (non-Tauri).

import { openUrl } from "@tauri-apps/plugin-opener";

/** True when running inside the Tauri runtime (vs. plain browser dev). */
function isTauri(): boolean {
  return globalThis.window !== undefined && "__TAURI_INTERNALS__" in globalThis.window;
}

/**
 * Open `url` in the user's default system browser.
 *
 * In a Tauri context this calls the `opener` plugin (subject to the
 * `opener:allow-open-url` capability). In a plain browser fallback this
 * routes to `window.open` for parity during dev preview.
 *
 * Logs (and swallows) errors so callers don't have to wrap each invocation
 * in try/catch - outbound link failures are non-critical and the worst case
 * is the user manually copy-pasting the URL.
 */
export async function openExternalUrl(url: string): Promise<void> {
  if (!url) return;
  try {
    if (isTauri()) {
      await openUrl(url);
    } else {
      window.open(url, "_blank", "noopener,noreferrer");
    }
  } catch (err) {
    console.error("[externalLink] failed to open URL", url, err);
  }
}

/**
 * Decide which URL a click on `anchor` should hand to the opener, or `null`
 * when the click must be left to the browser.
 *
 * Returns `null` for a modified click (meta/ctrl/shift/alt or a non-left
 * button), for an anchor with no href, and for a same-origin anchor so
 * in-app routing is never hijacked.
 *
 * Shared by the two entry points below so every call site applies the same
 * policy: the per-anchor handler, and the delegation used for anchors that
 * no component owns.
 */
export function resolveExternalHref(
  event: MouseEvent,
  anchor: HTMLAnchorElement,
): string | null {
  // Respect modified clicks (open-in-new-window etc.) - the user explicitly
  // wants the browser shortcut behaviour, don't override.
  if (
    event.metaKey ||
    event.ctrlKey ||
    event.shiftKey ||
    event.altKey ||
    event.button !== 0
  ) {
    return null;
  }
  const href = anchor.href;
  if (!href) return null;
  // Skip in-app navigation (same-origin anchor) so internal routing isn't
  // hijacked. Only true external URLs go through the opener.
  try {
    const target = new URL(href);
    if (target.origin === globalThis.location.origin) {
      return null;
    }
  } catch {
    return null;
  }
  return href;
}

/**
 * Svelte click handler that intercepts a plain `<a href>` click and routes
 * it through the opener plugin. Designed to be assigned with
 * `onclick={handleExternalLinkClick}` on the anchor.
 *
 * Skips middle-click, ctrl/cmd/shift-click (the user's intent there is
 * out-of-our-hands anyway) and anchors with no href.
 */
export function handleExternalLinkClick(
  event: MouseEvent & { currentTarget: EventTarget & HTMLAnchorElement },
): void {
  const href = resolveExternalHref(event, event.currentTarget);
  if (href === null) return;
  event.preventDefault();
  void openExternalUrl(href);
}
