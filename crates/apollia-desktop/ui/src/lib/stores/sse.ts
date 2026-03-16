/**
 * Runtime state store for Apollia Desktop.
 *
 * Uses Tauri IPC (invoke) exclusively — no direct HTTP connections.
 * This avoids CORS issues in dev mode (Vite on :5173) and is the correct
 * architecture for a Tauri app where all backend comms go through the bridge.
 *
 * Data is refreshed on a polling interval and on-demand after user actions.
 * The `connectionStatus` reflects whether the last IPC batch succeeded.
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
  LlmBackendStatus,
  TriggerStatus,
  PipelineRunSummary,
} from "$lib/types";

const POLL_INTERVAL_MS = 3_000;

/** Current connection status (reflects last IPC batch). */
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

// ─── IPC refresh helpers ──────────────────────────────────────────────────────

async function refreshAgentsViaIpc(): Promise<void> {
  try {
    const result: AgentStatus[] = await invoke("list_agents");
    agents.set(result);
    emitTrayUpdate();
  } catch {
    // runtime not ready yet — keep current state
  }
}

async function refreshTasksViaIpc(): Promise<void> {
  try {
    const result: TaskSummary[] = await invoke("list_tasks", { filter: null });
    tasks.set(result);
  } catch {
    // runtime not ready yet — keep current state
  }
}

async function refreshLlmBackendsViaIpc(): Promise<void> {
  try {
    const result: LlmBackendStatus[] = await invoke("list_llm_backends");
    llmBackends.set(result);
  } catch {
    // runtime not ready yet — keep current state
  }
}

async function refreshTriggersViaIpc(): Promise<void> {
  try {
    const result: TriggerStatus[] = await invoke("list_triggers");
    triggers.set(result);
  } catch {
    // runtime not ready yet — keep current state
  }
}

async function refreshPipelineRunsViaIpc(): Promise<void> {
  try {
    const result: PipelineRunSummary[] = await invoke("list_all_pipeline_runs", { limit: 50 });
    pipelineRuns.set(result);
  } catch {
    // runtime not ready yet — keep current state
  }
}

async function refreshPendingApprovalsViaIpc(): Promise<void> {
  try {
    const result: PendingApproval[] = await invoke("list_pending_approvals");
    const previous = get(pendingApprovals);
    const previousIds = new Set(previous.map((a) => a.task_id));

    // Send native notification for newly appeared approvals.
    for (const approval of result) {
      if (!previousIds.has(approval.task_id)) {
        void sendNativeNotification(approval.task_id);
      }
    }

    pendingApprovals.set(result);
    emitTrayUpdate();
  } catch {
    // runtime not ready yet — keep current state
  }
}

// ─── Tray sync ────────────────────────────────────────────────────────────────

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

// ─── Native notifications ─────────────────────────────────────────────────────

async function sendNativeNotification(taskId: string): Promise<void> {
  try {
    let granted = await isPermissionGranted();
    if (!granted) {
      const permission = await requestPermission();
      granted = permission === "granted";
    }
    if (!granted) return;
    sendNotification({
      title: "Action requise — Apollia OS",
      body: `Tâche ${taskId.slice(0, 8)} attend votre approbation`,
    });
  } catch {
    // Notification API unavailable — silently ignore
  }
}

// ─── Public API ───────────────────────────────────────────────────────────────

/**
 * Refresh all stores once via IPC. Call this after any user action that
 * changes runtime state (start agent, submit task, resume approval, etc.).
 */
export async function refreshAll(): Promise<void> {
  await Promise.allSettled([
    refreshAgentsViaIpc(),
    refreshTasksViaIpc(),
    refreshLlmBackendsViaIpc(),
    refreshTriggersViaIpc(),
    refreshPipelineRunsViaIpc(),
    refreshPendingApprovalsViaIpc(),
  ]);
}

/**
 * Start the polling loop. Returns a cleanup function that stops polling.
 *
 * Replaces the previous EventSource HTTP connection — Tauri apps must not
 * make direct HTTP requests from the WebView (CORS + no SSE in prod bundle).
 */
export function createSSEConnection(): () => void {
  let destroyed = false;
  let timer: ReturnType<typeof setTimeout> | null = null;

  async function poll(): Promise<void> {
    if (destroyed) return;

    try {
      await refreshAll();
      connectionStatus.set("connected");
    } catch {
      connectionStatus.set("reconnecting");
    }

    if (!destroyed) {
      timer = setTimeout(() => void poll(), POLL_INTERVAL_MS);
    }
  }

  // First poll immediately, then every POLL_INTERVAL_MS.
  connectionStatus.set("connecting");
  void poll();

  return () => {
    destroyed = true;
    if (timer !== null) {
      clearTimeout(timer);
      timer = null;
    }
    connectionStatus.set("connecting");
  };
}
