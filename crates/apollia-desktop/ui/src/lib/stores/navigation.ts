import { writable, derived, get, type Writable } from "svelte/store";
import { DEFAULT_ROUTE } from "$lib/navigation/developerRoutes";
import { openSettingsWindow, requestCloseSettingsWindow } from "$lib/stores/settingsWindow";

/** Routes available in the desktop application. */
export type Route = "dashboard" | "agents" | "tasks" | "chat" | "inbox" | "integrations" | "llm" | "automations" | "memory" | "transcriptions" | "notifications" | "observability" | "settings" | "design" | "design-motion" | "design-empty-states" | "design-dark-mode";

const screen = writable<Route>(DEFAULT_ROUTE);

/**
 * Reactive store of the active route, `DEFAULT_ROUTE` at launch.
 *
 * `settings` is an entry point, not a screen: the settings open in a window
 * above the current screen, which stays mounted underneath. Setting the route
 * to `settings`, by any path, opens that window and leaves the screen as it is.
 */
export const currentRoute: Writable<Route> = {
  subscribe: screen.subscribe,
  set(route: Route) {
    if (route === "settings") {
      openSettingsWindow();
      return;
    }
    screen.set(route);
  },
  update(updater: (route: Route) => Route) {
    currentRoute.set(updater(get(screen)));
  },
};

/** Navigation history for back/forward. */
const pastRoutes = writable<Route[]>([]);
const futureRoutes = writable<Route[]>([]);

export const canGoBack = derived(pastRoutes, ($past) => $past.length > 0);
export const canGoForward = derived(futureRoutes, ($future) => $future.length > 0);

/** Previous route if any (last entry of past stack). */
export const previousRoute = derived(pastRoutes, ($past) =>
  $past.length > 0 ? $past.at(-1) ?? null : null,
);
/** Next route if any (last entry of future stack). */
export const nextRoute = derived(futureRoutes, ($future) =>
  $future.length > 0 ? $future.at(-1) ?? null : null,
);
/** Size of the back history (for long-press tooltip "Back (N in history)"). */
export const backHistorySize = derived(pastRoutes, ($past) => $past.length);

/**
 * Navigate to a route, pushing current to history. `settings` opens the
 * settings window above the current screen; any other route closes that window
 * first, through its unsaved-changes guard, and goes on once it is closed.
 */
export function navigateTo(route: Route) {
  if (route === "settings") {
    openSettingsWindow();
    return;
  }
  requestCloseSettingsWindow(() => showScreen(route));
}

function showScreen(route: Route) {
  const current = get(screen);
  if (route === current) return;
  pastRoutes.update((p) => [...p, current].slice(-10));
  futureRoutes.set([]);
  screen.set(route);
}

/** Go back in history. */
export function goBack() {
  const past = get(pastRoutes);
  if (past.length === 0) return;
  const prev = past.at(-1);
  if (prev === undefined) return;
  const current = get(screen);
  pastRoutes.set(past.slice(0, -1));
  futureRoutes.update((f) => [...f, current]);
  currentRoute.set(prev);
}

/** Go forward in history. */
export function goForward() {
  const future = get(futureRoutes);
  if (future.length === 0) return;
  const next = future.at(-1);
  if (next === undefined) return;
  const current = get(screen);
  futureRoutes.set(future.slice(0, -1));
  pastRoutes.update((p) => [...p, current]);
  currentRoute.set(next);
}

// Sidebar state is owned by `./layout.ts` since (breakpoint-derived
// three-way state `expanded | icon | drawer`, persisted under `apollia.ui.sidebar`).
