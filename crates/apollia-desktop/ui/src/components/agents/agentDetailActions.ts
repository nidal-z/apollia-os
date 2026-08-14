/** Runtime states reported by `list_agents` for a loaded registry entry. */
export type RuntimeState =
  | "active"
  | "degraded"
  | "stopped"
  | "initializing"
  | "stopping";

/**
 * Whether the dashboard detail sheet offers the Start action.
 *
 * Stopping an agent keeps its registry entry alive with `runtime_status:
 * "stopped"`, so a non-null status does not mean the agent is running.
 * Start is offered when the agent is installed with a known path and its
 * runtime entry is either absent (never loaded) or stopped.
 */
export function canStartAgent(
  runtimeStatus: RuntimeState | null,
  installed: boolean,
  installPath: string | null,
): boolean {
  const idle = runtimeStatus === null || runtimeStatus === "stopped";
  return idle && installed && installPath !== null;
}
