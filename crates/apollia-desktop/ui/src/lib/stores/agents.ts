/**
 * Derived agent stores for Apollia Desktop.
 *
 * Re-exports the base agents store from sse.ts and provides derived
 * stores for filtered views (active count, degraded agents, etc.).
 */
import { derived } from "svelte/store";
import { agents } from "./sse";

export { agents } from "./sse";

/** Number of agents currently in 'active' state. */
export const activeAgentCount = derived(agents, ($agents) =>
  $agents.filter((a) => a.state === "active").length,
);

/** Agents currently in 'degraded' state. */
export const degradedAgents = derived(agents, ($agents) =>
  $agents.filter((a) => a.state === "degraded"),
);
