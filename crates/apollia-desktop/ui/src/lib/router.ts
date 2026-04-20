import { navigateTo, type Route } from "$lib/stores/navigation";
import {
  settingsSubRoute,
  goToSettingsSubRoute,
  SETTINGS_SUB_ROUTES,
  DEFAULT_SETTINGS_SUB_ROUTE,
  type SettingsSubRoute,
} from "$lib/stores/settings";

export { SETTINGS_SUB_ROUTES, DEFAULT_SETTINGS_SUB_ROUTE };
export type { SettingsSubRoute };

/** Parse a path like `/settings/stt` → the matching sub-route (or null). */
export function parseSettingsPath(path: string): SettingsSubRoute | null {
  const m = path.match(/^\/?settings(?:\/([^/?#]+))?/);
  if (!m) return null;
  const sub = m[1];
  if (!sub) return null;
  return (SETTINGS_SUB_ROUTES as string[]).includes(sub) ? (sub as SettingsSubRoute) : null;
}

/**
 * Navigate to `/settings` or `/settings/:sub`. Handles the redirect
 * `/settings` → `/settings/appearance` so callers don't have to.
 */
export function navigateToSettings(sub?: string | SettingsSubRoute | null): SettingsSubRoute {
  const resolved = goToSettingsSubRoute(sub ?? DEFAULT_SETTINGS_SUB_ROUTE);
  navigateTo("settings" as Route);
  return resolved;
}

/** Update the active settings sub-route without touching the top-level route. */
export function setSettingsSubRoute(sub: string | SettingsSubRoute): SettingsSubRoute {
  return goToSettingsSubRoute(sub);
}

export { settingsSubRoute };
