import { writable } from "svelte/store";

/** Routes disponibles dans l'application desktop. */
export type Route = "agents" | "tasks" | "approvals" | "llm" | "triggers" | "pipelines" | "memory";

/** Store réactif de la route active. Default = 'agents'. */
export const currentRoute = writable<Route>("agents");
