/**
 * Metadata for each top-level route - used by the Topbar breadcrumbs
 * and back/forward tooltips.
 *
 * - `labelKey`   : i18n key rendered in the breadcrumb segment.
 * - `parent`     : parent route (null for roots). Walked recursively to
 *                  build the full trail.
 * - `icon`       : lucide icon component rendered next to the label.
 * - `homeFor`    : UI modes for which this route is the home. Used by the
 *                  Topbar logo to route to the right landing page.
 */
// `ComponentType` is the Svelte 4 component class type. Svelte 5 introduced a
// new `Component` shape, but our route metadata still uses the legacy class
// type because the icons come from `lucide-svelte` (compiled with Svelte 4).
import type { ComponentType } from "svelte"; // NOSONAR typescript:S1874
import {
  LayoutDashboard,
  Bot,
  ListChecks,
  MessageSquare,
  ShieldCheck,
  Plug,
  Brain,
  Repeat,
  Database,
  Bell,
  Activity,
  Settings,
  Mic,
  FolderKanban,
  Paintbrush,
} from "lucide-svelte";
import type { Route } from "$lib/stores/navigation";

export interface RouteMeta {
  labelKey: string;
  parent: Route | null;
  icon?: ComponentType; // NOSONAR typescript:S1874
  /** Home route for the given UI mode(s). */
  homeFor?: Array<"operator" | "builder">;
}

export const routeMeta: Record<Route, RouteMeta> = {
  dashboard: { labelKey: "nav.dashboard", parent: null, icon: LayoutDashboard, homeFor: ["operator", "builder"] },
  agents: { labelKey: "nav.agents", parent: null, icon: Bot },
  projects: { labelKey: "nav.projects", parent: null, icon: FolderKanban },
  tasks: { labelKey: "nav.tasks", parent: null, icon: ListChecks },
  chat: { labelKey: "nav.chat", parent: null, icon: MessageSquare },
  inbox: { labelKey: "nav.inbox", parent: null, icon: ShieldCheck },
  integrations: { labelKey: "nav.connections", parent: null, icon: Plug },
  llm: { labelKey: "nav.llm", parent: null, icon: Brain },
  automations: { labelKey: "nav.automations", parent: null, icon: Repeat },
  memory: { labelKey: "nav.memory", parent: null, icon: Database },
  transcriptions: { labelKey: "nav.transcriptions", parent: null, icon: Mic },
  notifications: { labelKey: "nav.notifications", parent: null, icon: Bell },
  observability: { labelKey: "nav.observability", parent: null, icon: Activity },
  settings: { labelKey: "nav.settings", parent: null, icon: Settings },
  onboarding: { labelKey: "nav.onboarding", parent: null },
  design: { labelKey: "topbar.route.design", parent: null, icon: Paintbrush },
  "design-motion": { labelKey: "topbar.route.design_motion", parent: "design", icon: Paintbrush },
  "design-empty-states": { labelKey: "topbar.route.design_empty_states", parent: "design", icon: Paintbrush },
  "design-dark-mode": { labelKey: "topbar.route.design_dark_mode", parent: "design", icon: Paintbrush },
};

/** Walk the parent chain and return the full trail (root → current). */
export function buildTrail(route: Route): Route[] {
  const trail: Route[] = [];
  let cur: Route | null = route;
  const seen = new Set<Route>();
  while (cur && !seen.has(cur)) {
    seen.add(cur);
    trail.unshift(cur);
    cur = routeMeta[cur]?.parent ?? null;
  }
  return trail;
}

/** Return the home route for a given UI mode. */
export function homeRouteFor(mode: "operator" | "builder"): Route {
  const entry = (Object.entries(routeMeta) as Array<[Route, RouteMeta]>).find(
    ([, meta]) => meta.homeFor?.includes(mode),
  );
  return entry?.[0] ?? "dashboard";
}
