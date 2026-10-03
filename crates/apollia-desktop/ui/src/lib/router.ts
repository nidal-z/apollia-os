import { openSettingsWindow } from "$lib/stores/settingsWindow";
import {
  goToSettingsSubRoute,
  SETTINGS_SUB_ROUTES,
  DEFAULT_SETTINGS_SUB_ROUTE,
  type SettingsSubRoute,
} from "$lib/stores/settings";

export { SETTINGS_SUB_ROUTES, DEFAULT_SETTINGS_SUB_ROUTE } from "$lib/stores/settings";
export type { SettingsSubRoute } from "$lib/stores/settings";

/** Parse a path like `/settings/stt` → the matching sub-route (or null). */
export function parseSettingsPath(path: string): SettingsSubRoute | null {
  const m = /^\/?settings(?:\/([^/?#]+))?/.exec(path);
  if (!m) return null;
  const sub = m[1];
  if (!sub) return null;
  return (SETTINGS_SUB_ROUTES as string[]).includes(sub) ? (sub as SettingsSubRoute) : null;
}

/**
 * Open the settings window on a section (General by default), above the current
 * screen. A former section name lands on the section that holds its content.
 * `focus` names an element the section scrolls to and focuses once shown.
 */
export function navigateToSettings(
  sub?: string | SettingsSubRoute | null, // NOSONAR typescript:S6571
  focus?: string | null,
): SettingsSubRoute {
  return openSettingsWindow(sub ?? DEFAULT_SETTINGS_SUB_ROUTE, focus);
}

/** Update the active settings sub-route without touching the top-level route. */
export function setSettingsSubRoute(sub: string | SettingsSubRoute): SettingsSubRoute { // NOSONAR typescript:S6571
  return goToSettingsSubRoute(sub);
}

export { settingsSubRoute } from "$lib/stores/settings";
