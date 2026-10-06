/**
 * The screens reserved to the developer mode, and the switch that keeps them
 * out of reach while the mode is off.
 *
 * The sidebar lists these routes in its developer group, the palette flags
 * them, and the main router sends a request for one of them, mode off, to the
 * default route instead of rendering it. A request for a route no screen
 * answers to goes the same way.
 */

import type { Route } from "$lib/stores/navigation";
import { routeMeta } from "$lib/navigation/routeMeta";

/**
 * The route the application opens on, and the fallback of the router: the
 * chat, a screen where work is done.
 */
export const DEFAULT_ROUTE: Route = "chat";

/** The screens shown only with the developer mode on, in sidebar order. */
export const DEVELOPER_ROUTES: readonly Route[] = ["transcriptions", "observability", "memory"];

/** Whether `route` is reserved to the developer mode. */
export function isDeveloperRoute(route: Route): boolean {
  return DEVELOPER_ROUTES.includes(route);
}

/**
 * Whether a screen answers to `route`. The settings are a window above the
 * screen and never a screen; the design reference exists in a development
 * build only; a name outside the registry (a removed route, a link the guide
 * invented) has nothing to show.
 */
function hasScreen(route: Route, developmentBuild: boolean): boolean {
  if (!Object.hasOwn(routeMeta, route) || route === "settings") return false;
  return developmentBuild || !route.startsWith("design");
}

/**
 * The route the router renders for a requested one: the request itself, unless
 * no screen answers to it, or it names a developer screen while the mode is
 * off. Both fall back to the default route, so the window is never empty.
 */
export function reachableRoute(
  route: Route,
  developerMode: boolean,
  developmentBuild = false,
): Route {
  if (!hasScreen(route, developmentBuild)) return DEFAULT_ROUTE;
  return developerMode || !isDeveloperRoute(route) ? route : DEFAULT_ROUTE;
}
