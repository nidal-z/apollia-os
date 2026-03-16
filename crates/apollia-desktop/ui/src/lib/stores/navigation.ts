import { writable } from "svelte/store";

/** Routes disponibles dans l'application desktop. */
export type Route = "dashboard" | "agents" | "tasks" | "approvals" | "llm" | "triggers" | "pipelines" | "memory" | "notifications" | "observability" | "settings";

/** Store réactif de la route active. Default = 'dashboard'. */
export const currentRoute = writable<Route>("dashboard");
