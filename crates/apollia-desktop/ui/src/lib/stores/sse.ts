/**
 * SSE connection store for Apollia Desktop.
 *
 * Connects to the runtime dashboard SSE stream, dispatches events to
 * reactive Svelte stores, and handles reconnection with exponential backoff.
 */
import { writable } from "svelte/store";
import type {
  AgentStatus,
  TaskSummary,
  PendingApproval,
  ConnectionStatus,
  DashboardState,
  DashboardAgent,
  DashboardTask,
  HitlInputRequiredPayload,
  HitlResumedPayload,
} from "$lib/types";

const SSE_URL = "http://localhost:7771/api/v1/dashboard/stream";
const STATE_URL = "http://localhost:7771/api/v1/dashboard/state";
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
 * Fetch the full dashboard state snapshot from the runtime REST API.
 *
 * Updates agents and tasks stores with current data. Errors are logged
 * but do not throw — the SSE stream remains the primary data source.
 */
async function fetchDashboardState(): Promise<void> {
  try {
    const response = await fetch(STATE_URL);
    if (!response.ok) {
      return;
    }
    const state: DashboardState = await response.json();

    agents.set(
      state.agents.map((a: DashboardAgent): AgentStatus => ({
        id: a.id,
        name: a.id,
        state: normalizeAgentState(a.status),
        uptime_secs: 0,
        tasks_completed: 0,
        tasks_failed: 0,
      })),
    );

    tasks.set(
      state.recent_tasks.map((t: DashboardTask): TaskSummary => ({
        id: t.id,
        agent_id: t.agent,
        agent_name: t.agent,
        status: normalizeTaskStatus(t.status),
        input_preview: "",
        created_at: t.started_at,
      })),
    );
  } catch {
    // Network error — silently ignore, SSE will retry
  }
}

/** Normalize raw agent state string to typed union. */
function normalizeAgentState(raw: string): AgentStatus["state"] {
  const valid: AgentStatus["state"][] = [
    "initializing",
    "active",
    "degraded",
    "stopping",
    "stopped",
  ];
  return valid.includes(raw as AgentStatus["state"])
    ? (raw as AgentStatus["state"])
    : "stopped";
}

/** Normalize raw task status string to typed union. */
function normalizeTaskStatus(raw: string): TaskSummary["status"] {
  const valid: TaskSummary["status"][] = [
    "submitted",
    "working",
    "completed",
    "failed",
    "input_required",
    "canceled",
  ];
  return valid.includes(raw as TaskSummary["status"])
    ? (raw as TaskSummary["status"])
    : "submitted";
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
      // Fetch full state on (re)connect to hydrate stores
      void fetchDashboardState();
    };

    // Named event: agents channel — refetch agent list
    source.addEventListener("agents", () => {
      void fetchDashboardState();
    });

    // Named event: tasks channel — refetch task list
    source.addEventListener("tasks", () => {
      void fetchDashboardState();
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
