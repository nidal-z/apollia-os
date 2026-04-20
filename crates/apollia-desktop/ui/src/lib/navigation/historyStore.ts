/**
 * Thin facade over `$lib/stores/navigation` for topbar consumers
 * (US-SP42-077). The core history state (past/future stacks, bounded
 * to 10) still lives in the navigation store — this module only adds
 * derived labels used by the back/forward tooltips.
 */
import { derived } from "svelte/store";
import { get } from "svelte/store";
import {
  previousRoute,
  nextRoute,
  backHistorySize,
  type Route,
} from "$lib/stores/navigation";
import { routeMeta } from "./routeMeta";

export { previousRoute, nextRoute, backHistorySize };

/** i18n key for the label of a given route, or null if unknown. */
export function labelKeyFor(route: Route | null): string | null {
  if (!route) return null;
  return routeMeta[route]?.labelKey ?? null;
}

/** Derived store: i18n key of the previous route (for tooltip). */
export const previousLabelKey = derived(previousRoute, ($r) => labelKeyFor($r));
/** Derived store: i18n key of the next route (for tooltip). */
export const nextLabelKey = derived(nextRoute, ($r) => labelKeyFor($r));

/** Snapshot accessor (for tests/telemetry). */
export function snapshotHistory(): { back: Route | null; forward: Route | null; backSize: number } {
  return {
    back: get(previousRoute),
    forward: get(nextRoute),
    backSize: get(backHistorySize),
  };
}
