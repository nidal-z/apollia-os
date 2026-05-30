import { writable, derived, get } from "svelte/store";

/** Routes disponibles dans l'application desktop. */
export type Route = "dashboard" | "agents" | "tasks" | "chat" | "inbox" | "integrations" | "llm" | "automations" | "projects" | "memory" | "transcriptions" | "notifications" | "observability" | "settings" | "settings-permission-rules" | "design" | "design-motion" | "design-empty-states" | "design-dark-mode" | "onboarding";

/** Store réactif de la route active. Default = 'dashboard'. */
export const currentRoute = writable<Route>("dashboard");

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

/** Navigate to a route, pushing current to history. */
export function navigateTo(route: Route) {
  const current = get(currentRoute);
  if (route === current) return;
  pastRoutes.update((p) => [...p, current].slice(-10));
  futureRoutes.set([]);
  currentRoute.set(route);
}

/** Go back in history. */
export function goBack() {
  const past = get(pastRoutes);
  if (past.length === 0) return;
  const prev = past.at(-1);
  if (prev === undefined) return;
  const current = get(currentRoute);
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
  const current = get(currentRoute);
  futureRoutes.set(future.slice(0, -1));
  pastRoutes.update((p) => [...p, current]);
  currentRoute.set(next);
}

// Sidebar state is owned by `./layout.ts` since (breakpoint-derived
// three-way state `expanded | icon | drawer`, persisted under `apollia.ui.sidebar`).
