/**
 * The hotkey the recording overlay is opened with, read from its own address.
 *
 * The overlay window is built with `overlay.html?hotkey=<encoded>` so the page
 * knows the label before any event reaches it. The `stt-overlay-config` event
 * that follows a show is not enough on its own: the onboarding builds the
 * window and starts the first recording within the same second, and that
 * event left before the page had registered its listener, leaving the
 * placeholder on screen for the whole recording (measured 2026-09-09).
 *
 * Returns `null` when the address carries no hotkey, or an empty one, so the
 * caller keeps its placeholder rather than printing an empty label.
 */
export function hotkeyFromSearch(search: string): string | null {
  const value = new URLSearchParams(search).get("hotkey");
  if (value === null) return null;
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : null;
}
