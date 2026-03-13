/**
 * SSE connection store for Apollia Desktop.
 *
 * Connects to the runtime dashboard SSE stream, dispatches events to
 * reactive Svelte stores, and handles reconnection with exponential backoff.
 */
import { writable } from "svelte/store";
import { invoke } from "@tauri-apps/api/core";
import type {
  AgentStatus,
  TaskSummary,
  PendingApproval,
  ConnectionStatus,
  HitlInputRequiredPayload,
  HitlResumedPayload,
} from "$lib/types";

const SSE_URL = "http://localhost:7771/api/v1/dashboard/stream";
const INITIAL_RECONNECT_DELAY_MS = 1_000;
const MAX_RECONNECT_DELAY_MS = 30_000;

/** Current SSE connection status. */
export const connectionStatus = writable<ConnectionStatus>("connecting");

/** List of all agents from the runtime. */
export const agents = writable<AgentStatus[]>([]);

/** List of recent tasks from the runtime. */
export const tasks = writable<TaskSummary[]>([]);

/** List of pending HITL approvals. */
export const pendingApprovals = writable<PendingApproval[]>([]);

/**
 * Refresh the agents store using the Tauri IPC `list_agents` command.
 *
 * This provides richer data (name, uptime, task stats, degraded_reason)
 * than the dashboard REST snapshot. Called on SSE agent events and on connect.
 */
async function refreshAgentsViaIpc(): Promise<void> {
  try {
    const result: AgentStatus[] = await invoke("list_agents");
    agents.set(result);
  } catch {
    // IPC not available yet — fall back to dashboard state
  }
}

/**
 * Refresh the tasks store using the Tauri IPC `list_tasks` command.
 *
 * Called on SSE task events and on connect for richer data.
 */
async function refreshTasksViaIpc(): Promise<void> {
  try {
    const result: TaskSummary[] = await invoke("list_tasks", { filter: null });
    tasks.set(result);
  } catch {
    // IPC not available yet — fall back to dashboard state
  }
}

/** Handle an HITL approval event received via SSE. */
function handleApprovalEvent(data: string): void {
  let payload: HitlInputRequiredPayload | HitlResumedPayload;
  try {
    payload = JSON.parse(data);
  } catch {
    return;
  }

  if (payload.event_type === "TaskInputRequired") {
    const req = payload as HitlInputRequiredPayload;
    pendingApprovals.update((current) => {
      if (current.some((a) => a.task_id === req.task_id)) {
        return current;
      }
      return [
        ...current,
        {
          task_id: req.task_id,
          agent_name: "",
          prompt: req.prompt,
          suspended_at: new Date().toISOString(),
        },
      ];
    });
  } else if (payload.event_type === "TaskResumed") {
    const resumed = payload as HitlResumedPayload;
    pendingApprovals.update((current) =>
      current.filter((a) => a.task_id !== resumed.task_id),
    );
  }
}

/**
 * Initialise la connexion SSE avec reconnexion automatique.
 *
 * Returns a cleanup function that closes the connection and cancels
 * any pending reconnection timer.
 */
export function createSSEConnection(): () => void {
  let source: EventSource | null = null;
  let reconnectDelay = INITIAL_RECONNECT_DELAY_MS;
  let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  let destroyed = false;

  function connect(): void {
    if (destroyed) return;

    connectionStatus.set(
      reconnectDelay > INITIAL_RECONNECT_DELAY_MS ? "reconnecting" : "connecting",
    );

    source = new EventSource(SSE_URL);

    source.onopen = () => {
      connectionStatus.set("connected");
      reconnectDelay = INITIAL_RECONNECT_DELAY_MS;
      // Hydrate stores via Tauri IPC for rich data
      void refreshAgentsViaIpc();
      void refreshTasksViaIpc();
    };

    // Named event: agents channel — refresh via IPC
    source.addEventListener("agents", () => {
      void refreshAgentsViaIpc();
    });

    // Named event: tasks channel — refresh via IPC
    source.addEventListener("tasks", () => {
      void refreshTasksViaIpc();
    });

    // Named event: approvals channel — parse JSON payload directly
    source.addEventListener("approvals", (event: MessageEvent<string>) => {
      handleApprovalEvent(event.data);
    });

    source.onerror = () => {
      connectionStatus.set("reconnecting");
      source?.close();
      source = null;
      scheduleReconnect();
    };
  }

  function scheduleReconnect(): void {
    if (destroyed) return;
    reconnectTimer = setTimeout(() => {
      reconnectTimer = null;
      connect();
    }, reconnectDelay);
    reconnectDelay = Math.min(reconnectDelay * 2, MAX_RECONNECT_DELAY_MS);
  }

  connect();

  return () => {
    destroyed = true;
    source?.close();
    source = null;
    if (reconnectTimer !== null) {
      clearTimeout(reconnectTimer);
      reconnectTimer = null;
    }
  };
}
