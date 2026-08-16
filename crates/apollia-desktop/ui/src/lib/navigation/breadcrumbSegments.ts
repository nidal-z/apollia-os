/**
 * Breadcrumb segment model consumed by `OperatorBreadcrumb.svelte`.
 *
 * The registry (`routeMeta`) only knows top-level routes, so a trail built
 * from it stops at `settings` whatever sub-page is open. Settings is the one
 * two-level area of the product: its open sub-page lives in the
 * `settingsSubRoute` store, and this module appends it to the trail as a
 * non-clickable leaf so the breadcrumb answers "where am I" again.
 */
// `ComponentType` is the Svelte 4 component class type, kept for lucide-svelte
// icons (compiled with Svelte 4). Same rationale as in `routeMeta.ts`.
import type { ComponentType } from "svelte"; // NOSONAR typescript:S1874
import type { Route } from "$lib/stores/navigation";
import type { SettingsSubRoute } from "$lib/stores/settings";
import { buildTrail, routeMeta } from "$lib/navigation/routeMeta";

export interface BreadcrumbSegment {
  /** i18n key rendered for the segment. */
  labelKey: string;
  /** Navigation target when the segment is clickable; null for the settings leaf. */
  route: Route | null;
  icon?: ComponentType; // NOSONAR typescript:S1874
}

/**
 * i18n key of a settings sub-page, mirroring the keys the settings navigation
 * column reads (`routes/Settings.svelte`): hyphens become underscores, so
 * "model-hub" resolves `settings.nav.model_hub`.
 */
export function settingsSubRouteLabelKey(sub: SettingsSubRoute): string {
  return `settings.nav.${sub.replace(/-/g, "_")}`;
}

/**
 * Full breadcrumb trail for a route: the registry parent chain, plus the open
 * settings sub-page as a leaf when the route is `settings`.
 */
export function buildBreadcrumbSegments(
  route: Route,
  settingsSub: SettingsSubRoute,
): BreadcrumbSegment[] {
  const segments: BreadcrumbSegment[] = buildTrail(route).map((r) => ({
    labelKey: routeMeta[r].labelKey,
    route: r,
    icon: routeMeta[r].icon,
  }));
  if (route === "settings") {
    segments.push({ labelKey: settingsSubRouteLabelKey(settingsSub), route: null });
  }
  return segments;
}
