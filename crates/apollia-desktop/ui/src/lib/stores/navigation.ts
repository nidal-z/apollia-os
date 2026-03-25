import { writable, derived, get } from "svelte/store";

/** Routes disponibles dans l'application desktop. */
export type Route = "dashboard" | "agents" | "tasks" | "chat" | "approvals" | "llm" | "triggers" | "pipelines" | "memory" | "transcriptions" | "notifications" | "observability" | "settings" | "onboarding";

/** Store réactif de la route active. Default = 'dashboard'. */
export const currentRoute = writable<Route>("dashboard");

/** Navigation history for back/forward. */
const pastRoutes = writable<Route[]>([]);
const futureRoutes = writable<Route[]>([]);

export const canGoBack = derived(pastRoutes, ($past) => $past.length > 0);
export const canGoForward = derived(futureRoutes, ($future) => $future.length > 0);

/** Navigate to a route, pushing current to history. */
export function navigateTo(route: Route) {
  const current = get(currentRoute);
  if (route === current) return;
  pastRoutes.update((p) => [...p, current]);
  futureRoutes.set([]);
  currentRoute.set(route);
}

/** Go back in history. */
export function goBack() {
  const past = get(pastRoutes);
  if (past.length === 0) return;
  const prev = past[past.length - 1];
  const current = get(currentRoute);
  pastRoutes.set(past.slice(0, -1));
  futureRoutes.update((f) => [...f, current]);
  currentRoute.set(prev);
}

/** Go forward in history. */
export function goForward() {
  const future = get(futureRoutes);
  if (future.length === 0) return;
  const next = future[future.length - 1];
  const current = get(currentRoute);
  futureRoutes.set(future.slice(0, -1));
  pastRoutes.update((p) => [...p, current]);
  currentRoute.set(next);
}

/** Sidebar collapsed state, persisted to localStorage. */
function createSidebarStore() {
  const stored = typeof localStorage !== "undefined" ? localStorage.getItem("apollia-sidebar-collapsed") : null;
  const { subscribe, set, update } = writable(stored === "true");

  return {
    subscribe,
    set(value: boolean) {
      set(value);
      if (typeof localStorage !== "undefined") {
        localStorage.setItem("apollia-sidebar-collapsed", String(value));
      }
    },
    toggle() {
      update((v) => {
        const next = !v;
        if (typeof localStorage !== "undefined") {
          localStorage.setItem("apollia-sidebar-collapsed", String(next));
        }
        return next;
      });
    },
  };
}

export const sidebarCollapsed = createSidebarStore();
