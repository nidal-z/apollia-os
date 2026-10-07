/**
 * The routes the guide knows: every screen a release build shows.
 *
 * The list is derived from `routeMeta`, which is keyed by the `Route` union,
 * so a route added to the union is offered to the guide without anything to
 * edit here, and needs its context text (`screenContexts.ts`) before the
 * build passes. The design reference is left out: only a development build
 * mounts it.
 */
import { isDeveloperRoute } from "$lib/navigation/developerRoutes";
import { routeMeta } from "$lib/navigation/routeMeta";
import type { Route } from "$lib/stores/navigation";

/** A route a release build shows. */
export type VisibleRoute = Exclude<Route, "design" | `design-${string}`>;

/** Whether `route` names a screen a release build shows. */
export function isVisibleRoute(route: string): route is VisibleRoute {
  return Object.hasOwn(routeMeta, route) && route !== "design" && !route.startsWith("design-");
}

/**
 * The visible routes for a given developer mode. With the mode off, the
 * developer screens are left out: the router would answer a request for one
 * of them with the default screen.
 */
export function visibleRoutes(developerMode = true): VisibleRoute[] {
  return Object.keys(routeMeta)
    .filter(isVisibleRoute)
    .filter((route) => developerMode || !isDeveloperRoute(route));
}
