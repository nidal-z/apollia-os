/**
 * Derived agent stores for Apollia Desktop.
 *
 * Re-exports the base agents store from sse.ts and provides derived
 * stores for filtered views (active count, degraded agents, installed, etc.).
 */
import { derived } from "svelte/store";
import { agents } from "./sse";

export { agents } from "./sse";

/** Number of agents currently in 'active' runtime state. */
export const activeAgentCount = derived(agents, ($agents) =>
  $agents.filter((a) => a.runtime_status === "active").length,
);

/** Agents currently in 'degraded' runtime state. */
export const degradedAgents = derived(agents, ($agents) =>
  $agents.filter((a) => a.runtime_status === "degraded"),
);

/** Agents that are installed (persisted in agents.db). */
export const installedAgents = derived(agents, ($agents) =>
  $agents.filter((a) => a.installed_at !== null),
);

/** Agents that are runtime-only (session-only, not installed). */
export const sessionOnlyAgents = derived(agents, ($agents) =>
  $agents.filter((a) => a.installed_at === null),
);
