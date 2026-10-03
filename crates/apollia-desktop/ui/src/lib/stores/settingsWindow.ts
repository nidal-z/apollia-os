/**
 * The settings window: the settings open above the current screen, in the same
 * webview, rather than replacing it. This store says whether the window is open
 * and on which section; the section itself stays in `settingsSubRoute`, so the
 * window reopens where it was left.
 *
 * Closing goes through a guard the open window installs: a section with typed
 * changes not yet saved holds the window open and asks first (stay, discard,
 * save). A close requested on the way to another screen carries that screen as
 * a continuation, run once the guard lets the window go.
 */
import { derived, get, writable } from "svelte/store";
import {
  goToSettingsSubRoute,
  requestSettingsSubRoute,
  settingsSubRoute,
  type SettingsSubRoute,
} from "$lib/stores/settings";

/** `true` while the settings window is shown above the current screen. */
export const settingsWindowOpen = writable(false);

/** The window as one value: open or not, and the section it shows. */
export const settingsWindow = derived(
  [settingsWindowOpen, settingsSubRoute],
  ([$open, $section]) => ({ open: $open, section: $section }),
);

/**
 * Element id a section scrolls to and focuses once mounted (the direct access
 * to one action of the danger zone). Read once, then cleared.
 */
const pendingFocus = writable<string | null>(null);

/** Take the pending focus target, if any, and clear it. */
export function consumeSettingsFocus(): string | null {
  const target = get(pendingFocus);
  if (target !== null) pendingFocus.set(null);
  return target;
}

/**
 * Open the window. With a section, show it: through the unsaved-changes guard
 * when the window is already open, directly otherwise. Without one, the window
 * reopens on the section it was left on. Returns the section shown.
 */
export function openSettingsWindow(
  section?: string | null,
  focus?: string | null,
): SettingsSubRoute {
  let shown: SettingsSubRoute;
  if (section === undefined || section === null) {
    shown = get(settingsSubRoute);
  } else if (get(settingsWindowOpen)) {
    shown = requestSettingsSubRoute(section) ?? get(settingsSubRoute);
  } else {
    shown = goToSettingsSubRoute(section);
  }
  pendingFocus.set(focus ?? null);
  settingsWindowOpen.set(true);
  return shown;
}

/**
 * Decides whether the window may close now. Returning `false` keeps it open:
 * the guard then asks the user, and calls `then` once the window is closed.
 */
type CloseGuard = (then: (() => void) | null) => boolean;
let closeGuard: CloseGuard | null = null;

/** Install the close guard of the open window. Returns an uninstall function. */
export function installSettingsCloseGuard(guard: CloseGuard): () => void {
  closeGuard = guard;
  return () => {
    if (closeGuard === guard) closeGuard = null;
  };
}

/** Close the window without consulting the guard (after discard or save). */
export function closeSettingsWindow(): void {
  pendingFocus.set(null);
  settingsWindowOpen.set(false);
}

/**
 * Close the window, consulting the guard. Returns `true` when it closed (and
 * `then` ran), `false` when the guard kept it open to ask first.
 */
export function requestCloseSettingsWindow(then: (() => void) | null = null): boolean {
  if (!get(settingsWindowOpen)) {
    then?.();
    return true;
  }
  if (closeGuard && !closeGuard(then)) return false;
  closeSettingsWindow();
  then?.();
  return true;
}
