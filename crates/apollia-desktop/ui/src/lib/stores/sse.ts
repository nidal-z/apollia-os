/**
 * SSE connection store for Apollia Desktop.
 *
 * Connects to the runtime dashboard SSE stream, dispatches events to
 * reactive Svelte stores, and handles reconnection with exponential backoff.
 *
 * STORY-151: Uses Tauri native notifications for HITL approvals instead of
 * the browser Notification API. Emits `tray-update` events to keep the
 * system tray badge and tooltip in sync.
 */
import { writable, get } from "svelte/store";
import { invoke } from "@tauri-apps/api/core";
import { emit } from "@tauri-apps/api/event";
import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from "@tauri-apps/plugin-notification";
import type {
  AgentStatus,
  TaskSummary,
  PendingApproval,
  ConnectionStatus,
  HitlInputRequiredPayload,
  HitlResumedPayload,
  LlmBackendStatus,
  TriggerStatus,
  PipelineRunSummary,
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

/** List of LLM backends from the runtime. */
export const llmBackends = writable<LlmBackendStatus[]>([]);

/** List of triggers from the runtime. */
export const triggers = writable<TriggerStatus[]>([]);

/** List of pipeline runs from the runtime. */
export const pipelineRuns = writable<PipelineRunSummary[]>([]);

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
    emitTrayUpdate();
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

/**
 * Refresh the LLM backends store using the Tauri IPC `list_llm_backends` command.
 *
 * Called on SSE "llm" events and on connect for up-to-date backend status.
 */
async function refreshLlmBackendsViaIpc(): Promise<void> {
  try {
    const result: LlmBackendStatus[] = await invoke("list_llm_backends");
    llmBackends.set(result);
  } catch {
    // IPC not available yet — keep current state
  }
}

/**
 * Refresh the triggers store using the Tauri IPC `list_triggers` command.
 *
 * Called on SSE "triggers" events and on connect for up-to-date trigger status.
 */
async function refreshTriggersViaIpc(): Promise<void> {
  try {
    const result: TriggerStatus[] = await invoke("list_triggers");
    triggers.set(result);
  } catch {
    // IPC not available yet — keep current state
  }
}

/**
 * Refresh the pipeline runs store using the Tauri IPC `list_all_pipeline_runs` command.
 *
 * Called on SSE "pipeline" events and on connect for up-to-date run status.
 */
async function refreshPipelineRunsViaIpc(): Promise<void> {
  try {
    const result: PipelineRunSummary[] = await invoke("list_all_pipeline_runs", { limit: 50 });
    pipelineRuns.set(result);
  } catch {
    // IPC not available yet — keep current state
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
    let isNew = false;
    pendingApprovals.update((current) => {
      if (current.some((a) => a.task_id === req.task_id)) {
        return current;
      }
      isNew = true;
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
    if (isNew) {
      void sendNativeNotification(req.task_id);
      emitTrayUpdate();
    }
  } else if (payload.event_type === "TaskResumed") {
    const resumed = payload as HitlResumedPayload;
    pendingApprovals.update((current) =>
      current.filter((a) => a.task_id !== resumed.task_id),
    );
    emitTrayUpdate();
  }
}

/**
 * Send a native Tauri notification when a new HITL approval arrives.
 *
 * Requests permission on first call if not yet granted. If the user denies
 * permission, notifications are silently skipped (AC-7).
 */
async function sendNativeNotification(taskId: string): Promise<void> {
  try {
    let granted = await isPermissionGranted();
    if (!granted) {
      const permission = await requestPermission();
      granted = permission === "granted";
    }
    if (!granted) {
      return;
    }
    sendNotification({
      title: "Action requise — Apollia OS",
      body: `Tâche ${taskId.slice(0, 8)} attend votre approbation`,
    });
  } catch {
    // Notification API unavailable — silently ignore (AC-7)
  }
}

/**
 * Emit a `tray-update` event to the Rust backend so the system tray tooltip
 * and menu item are updated with current agent/approval counts (AC-6).
 */
function emitTrayUpdate(): void {
  const currentAgents = get(agents);
  const currentApprovals = get(pendingApprovals);

  const activeAgents = currentAgents.filter(
    (a) => a.state === "active" || a.state === "degraded",
  ).length;

  void emit("tray-update", {
    active_agents: activeAgents,
    pending_approvals: currentApprovals.length,
  });
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
      void refreshLlmBackendsViaIpc();
      void refreshTriggersViaIpc();
      void refreshPipelineRunsViaIpc();
    };

    // Named event: agents channel — refresh via IPC
    source.addEventListener("agents", () => {
      void refreshAgentsViaIpc();
    });

    // Named event: tasks channel — refresh via IPC
    source.addEventListener("tasks", () => {
      void refreshTasksViaIpc();
    });

    // Named event: llm channel — refresh via IPC
    source.addEventListener("llm", () => {
      void refreshLlmBackendsViaIpc();
    });

    // Named event: triggers channel — refresh via IPC
    source.addEventListener("triggers", () => {
      void refreshTriggersViaIpc();
    });

    // Named event: pipeline channel — refresh via IPC
    source.addEventListener("pipeline", () => {
      void refreshPipelineRunsViaIpc();
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
