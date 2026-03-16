/**
 * Runtime event store for Apollia Desktop.
 *
 * Listens to Tauri events emitted by the Rust EventBus bridge (`events.rs`)
 * and dispatches to the appropriate Svelte stores.  A watchdog triggers a
 * full IPC refresh if no event is received within 10 seconds.
 *
 * Replaces the previous 3-second polling loop (STORY-156 / ADR-030).
 */
import { writable, get } from "svelte/store";
import { invoke } from "@tauri-apps/api/core";
import { listen, emit, type UnlistenFn } from "@tauri-apps/api/event";
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

/** Watchdog timeout — triggers a single IPC refresh if no event received. */
const WATCHDOG_TIMEOUT_MS = 10_000;

/** Current connection status (reflects event bridge health). */
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
    const result: PipelineRunSummary[] = await invoke(
      "list_all_pipeline_runs",
      { limit: 50 },
    );
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

// ─── Event dispatch ───────────────────────────────────────────────────────────

/** Payload shape emitted by the Rust event bridge (`TauriRuntimeEvent`). */
interface TauriRuntimeEvent {
  category: string;
  event_type: string;
  payload: Record<string, unknown>;
}

/**
 * Dispatches a runtime event to the appropriate store by refreshing the
 * relevant domain via IPC.  This ensures data consistency (the IPC command
 * returns the full current state, not just a delta).
 */
function dispatchEvent(event: TauriRuntimeEvent): void {
  switch (event.category) {
    case "agent-changed":
      void refreshAgentsViaIpc();
      break;
    case "task-changed":
      void refreshTasksViaIpc();
      break;
    case "approval-changed":
      void refreshPendingApprovalsViaIpc();
      void refreshTasksViaIpc();
      break;
    case "llm-changed":
      void refreshLlmBackendsViaIpc();
      break;
    case "trigger-fired":
      void refreshTriggersViaIpc();
      break;
    case "pipeline-changed":
      void refreshPipelineRunsViaIpc();
      break;
    case "system":
      // AllReady / ShutdownRequested / FatalError — refresh everything
      void refreshAll();
      break;
  }
}

// ─── Public API ───────────────────────────────────────────────────────────────

/**
 * Refresh all stores once via IPC.  Call this after any user action that
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
 * Start listening to Tauri runtime events.  Returns a cleanup function.
 *
 * 1. Calls `refreshAll()` once for initial hydration.
 * 2. Listens to `"runtime-event"` emitted by the Rust bridge.
 * 3. A watchdog timer triggers a single `refreshAll()` if no event arrives
 *    within 10 seconds (fallback for edge cases).
 */
export function createSSEConnection(): () => void {
  let destroyed = false;
  let unlistenFn: UnlistenFn | null = null;
  let watchdogTimer: ReturnType<typeof setTimeout> | null = null;

  function resetWatchdog(): void {
    if (watchdogTimer !== null) {
      clearTimeout(watchdogTimer);
    }
    if (destroyed) return;
    watchdogTimer = setTimeout(() => {
      if (destroyed) return;
      void refreshAll().then(() => {
        if (!destroyed) {
          connectionStatus.set("connected");
          resetWatchdog();
        }
      });
    }, WATCHDOG_TIMEOUT_MS);
  }

  // 1. Initial hydration
  connectionStatus.set("connecting");
  void refreshAll().then(() => {
    if (!destroyed) {
      connectionStatus.set("connected");
    }
  });

  // 2. Listen to Tauri events from the Rust bridge
  void listen<TauriRuntimeEvent>("runtime-event", (event) => {
    if (destroyed) return;
    connectionStatus.set("connected");
    dispatchEvent(event.payload);
    resetWatchdog();
  }).then((fn) => {
    if (destroyed) {
      fn();
    } else {
      unlistenFn = fn;
      resetWatchdog();
    }
  });

  // 3. Cleanup function
  return () => {
    destroyed = true;
    if (watchdogTimer !== null) {
      clearTimeout(watchdogTimer);
      watchdogTimer = null;
    }
    if (unlistenFn !== null) {
      unlistenFn();
      unlistenFn = null;
    }
    connectionStatus.set("connecting");
  };
}
