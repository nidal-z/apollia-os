/**
 * The screens reserved to the developer mode, and the switch that keeps them
 * out of reach while the mode is off.
 *
 * The sidebar lists these routes in its developer group, the palette flags
 * them, and the main router sends a request for one of them, mode off, to the
 * default route instead of rendering it.
 */

import type { Route } from "$lib/stores/navigation";

/** The route the application opens on, and the fallback of the router. */
export const DEFAULT_ROUTE: Route = "dashboard";

/** The screens shown only with the developer mode on, in sidebar order. */
export const DEVELOPER_ROUTES: readonly Route[] = ["transcriptions", "observability"];

/** Whether `route` is reserved to the developer mode. */
export function isDeveloperRoute(route: Route): boolean {
  return DEVELOPER_ROUTES.includes(route);
}

/**
 * The route the router renders for a requested one: the request itself, unless
 * it names a developer screen while the mode is off.
 */
export function reachableRoute(route: Route, developerMode: boolean): Route {
  return developerMode || !isDeveloperRoute(route) ? route : DEFAULT_ROUTE;
}
