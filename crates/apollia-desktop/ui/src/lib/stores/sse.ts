/**
 * Runtime event store for Apollia Desktop.
 *
 * Listens to Tauri events emitted by the Rust EventBus bridge (`events.rs`)
 * and dispatches to the appropriate Svelte stores.  A watchdog triggers a
 * full IPC refresh if no event is received within 10 seconds.
 *
 * Replaces the previous 3-second polling loop (ADR-030).
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
  AgentListItem,
  AgentMessage,
  TaskSummary,
  PendingApproval,
  ConnectionStatus,
  LlmBackendConfig,
  TriggerStatus,
  PipelineRunSummary,
  ChatSessionSummary,
  PlanCacheHitEvent,
  InsightEntry,
  RejectedInsightEntry,
} from "$lib/types";
import { onboardingStore } from "./onboarding";
import { currentRoute, navigateTo } from "./navigation";
import { refreshSttStatus, refreshTranscriptions } from "./stt";
import {
  appendGlobalToken,
  clearGlobalBuffer,
  addPendingChatApproval,
  removePendingChatApproval,
} from "./chat-global";

/** Watchdog timeout — triggers a single IPC refresh if no event received. */
const WATCHDOG_TIMEOUT_MS = 10_000;

/** Current connection status (reflects event bridge health). */
export const connectionStatus = writable<ConnectionStatus>("connecting");

/** List of all agents (installed + runtime). */
export const agents = writable<AgentListItem[]>([]);

/** List of recent tasks from the runtime. */
export const tasks = writable<TaskSummary[]>([]);

/** List of pending HITL approvals. */
export const pendingApprovals = writable<PendingApproval[]>([]);

/** List of LLM backends from the runtime. */
export const llmBackends = writable<LlmBackendConfig[]>([]);

/** List of triggers from the runtime. */
export const triggers = writable<TriggerStatus[]>([]);

/** List of pipeline runs from the runtime. */
export const pipelineRuns = writable<PipelineRunSummary[]>([]);

/** List of chat sessions from the runtime. */
export const chatSessions = writable<ChatSessionSummary[]>([]);

/** Dernier événement de cache hit reçu (`null` si aucun). */
export const lastPlanCacheHit = writable<PlanCacheHitEvent | null>(null);

/** Compteur cumulé de cache hits depuis le démarrage du store. */
export const planCacheHitCount = writable<number>(0);

/** Dernier message inter-agents reçu via SSE (`null` si aucun). */
export const lastAgentMessage = writable<AgentMessage | null>(null);

/** Insights extracted from the most recent session by LLM analysis. */
export const extractedInsights = writable<InsightEntry[]>([]);

/**
 * Rejected insights kept in-memory for audit (US-SP42-042). Populated when the
 * user rejects an insight with a reason; surfaced in the "Rejected" tab of
 * InsightsFeedback so the rationale remains visible.
 */
export const rejectedInsights = writable<RejectedInsightEntry[]>([]);

/** Real-time session LLM cost — updated on every TokenBudgetUpdated event. */
export const sessionBudget = writable<SessionBudgetState | null>(null);

/** Shape of the TokenBudgetUpdated event payload (Rust enum, externally tagged). */
export interface SessionBudgetState {
  session_cost_usd: number;
  total_input_tokens: number;
  total_output_tokens: number;
  total_cache_read_tokens: number;
  threshold_usd: number;
  threshold_exceeded: boolean;
}

// ─── IPC refresh helpers ──────────────────────────────────────────────────────

async function refreshAgentsViaIpc(): Promise<void> {
  try {
    const result: AgentListItem[] = await invoke("list_agents");
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
    const result: LlmBackendConfig[] = await invoke("list_llm_backends");
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

async function refreshChatSessionsViaIpc(): Promise<void> {
  try {
    const result: ChatSessionSummary[] = await invoke("list_chat_sessions");
    chatSessions.set(result);
  } catch {
    // runtime not ready yet — keep current state
  }
}

// ─── Tray sync ────────────────────────────────────────────────────────────────

function emitTrayUpdate(): void {
  const currentAgents = get(agents);
  const currentApprovals = get(pendingApprovals);

  const activeAgents = currentAgents.filter(
    (a) => a.runtime_status === "active" || a.runtime_status === "degraded",
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

async function sendChatApprovalNotification(
  sessionId: string,
  toolName: string,
): Promise<void> {
  try {
    let granted = await isPermissionGranted();
    if (!granted) {
      const permission = await requestPermission();
      granted = permission === "granted";
    }
    if (!granted) return;
    sendNotification({
      title: "Approbation requise — Chat",
      body: `L'outil « ${toolName} » demande votre autorisation (session ${sessionId.slice(0, 8)})`,
    });
  } catch {
    // Notification API unavailable — silently ignore
  }
}

async function sendToolFailureNotification(
  sessionId: string,
  toolName: string,
): Promise<void> {
  try {
    let granted = await isPermissionGranted();
    if (!granted) {
      const permission = await requestPermission();
      granted = permission === "granted";
    }
    if (!granted) return;
    sendNotification({
      title: "Outil échoué — Apollia OS",
      body: `L'outil « ${toolName} » a échoué (session ${sessionId.slice(0, 8)})`,
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
      if (event.event_type === "TokenBudgetUpdated") {
        // Direct store update — no IPC round-trip needed, payload carries all fields.
        const raw = event.payload as { TokenBudgetUpdated?: SessionBudgetState };
        const budget = raw.TokenBudgetUpdated;
        if (budget) {
          sessionBudget.set(budget);
        }
      } else {
        void refreshLlmBackendsViaIpc();
      }
      break;
    case "trigger-fired":
      void refreshTriggersViaIpc();
      break;
    case "pipeline-changed":
      void refreshPipelineRunsViaIpc();
      break;
    case "chat-changed":
      void refreshChatSessionsViaIpc();
      // ── Global chat approval / streaming tracking ──────────────────
      if (event.event_type === "ChatApprovalRequired") {
        const inner =
          (event.payload as Record<string, unknown>)?.ChatApprovalRequired as
            | {
                session_id?: string;
                message_id?: string;
                tool_name?: string;
                prompt?: string;
              }
            | undefined;
        const p = inner ?? (event.payload as Record<string, unknown>);
        if (p.session_id) {
          addPendingChatApproval({
            sessionId: String(p.session_id),
            messageId: String(p.message_id ?? ""),
            toolName: String(p.tool_name ?? ""),
            inputPreview: String(p.prompt ?? ""),
            receivedAt: new Date().toISOString(),
          });
          void sendChatApprovalNotification(
            String(p.session_id),
            String(p.tool_name ?? "tool"),
          );
        }
      } else if (
        event.event_type === "ChatApprovalResolved" ||
        event.event_type === "ChatApprovalTimeout"
      ) {
        const inner =
          (event.payload as Record<string, unknown>)?.[event.event_type] as
            | { session_id?: string; message_id?: string; tool_name?: string }
            | undefined;
        const p = inner ?? (event.payload as Record<string, unknown>);
        if (p.session_id) {
          removePendingChatApproval(
            String(p.session_id),
            p.message_id ? String(p.message_id) : undefined,
            p.tool_name ? String(p.tool_name) : undefined,
          );
        }
      } else if (event.event_type === "ChatResponseCompleted") {
        const inner =
          (event.payload as Record<string, unknown>)?.ChatResponseCompleted as
            | { session_id?: string }
            | undefined;
        const p = inner ?? (event.payload as Record<string, unknown>);
        if (p.session_id) {
          clearGlobalBuffer(String(p.session_id));
          // US-SP42-030 — refresh SessionMetrics (throttled inside store).
          void import("./chatMetrics").then((m) =>
            m.refreshSessionMetrics(String(p.session_id)),
          );
        }
      } else if (event.event_type === "ChatToolCallCompleted") {
        const inner =
          (event.payload as Record<string, unknown>)?.ChatToolCallCompleted as
            | {
                session_id?: string;
                tool_name?: string;
                success?: boolean;
                output_preview?: string;
              }
            | undefined;
        const p = inner ?? (event.payload as Record<string, unknown>);
        if (p.success === false && p.tool_name) {
          void sendToolFailureNotification(
            String(p.session_id ?? ""),
            String(p.tool_name),
          );
        }
        if (p.session_id) {
          void import("./chatMetrics").then((m) =>
            m.refreshSessionMetrics(String(p.session_id)),
          );
        }
      } else if (event.event_type === "ChatError") {
        const inner =
          (event.payload as Record<string, unknown>)?.ChatError as
            | { session_id?: string }
            | undefined;
        const p = inner ?? (event.payload as Record<string, unknown>);
        if (p.session_id) {
          clearGlobalBuffer(String(p.session_id));
        }
      }
      break;
    case "plan-cache-hit":
      lastPlanCacheHit.set(event.payload as unknown as PlanCacheHitEvent);
      planCacheHitCount.update((n) => n + 1);
      break;
    case "agent-message-sent":
      lastAgentMessage.set(event.payload as unknown as AgentMessage);
      void refreshAgentsViaIpc();
      break;
    case "memory-extraction": {
      const insights = (event.payload as { insights?: InsightEntry[] })
        .insights;
      if (insights && insights.length > 0) {
        extractedInsights.set(insights);
      }
      break;
    }
    case "stt-changed":
      void refreshSttStatus();
      void refreshTranscriptions();
      break;
    case "onboarding-required":
      onboardingStore.setRequired();
      if (!get(onboardingStore).completed) {
        navigateTo("onboarding");
      }
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
    refreshChatSessionsViaIpc(),
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
  let unlistenChatTokenFn: UnlistenFn | null = null;
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

  // 2b. Global chat-token listener — accumulates tokens into the global buffer
  //     so streaming data is never lost when the ChatConversation component
  //     is not mounted.
  void listen<{ session_id: string; message_id: string; token: string }>(
    "chat-token",
    (event) => {
      if (destroyed) return;
      appendGlobalToken(event.payload.session_id, event.payload.token);
    },
  ).then((fn) => {
    if (destroyed) {
      fn();
    } else {
      unlistenChatTokenFn = fn;
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
    if (unlistenChatTokenFn !== null) {
      unlistenChatTokenFn();
      unlistenChatTokenFn = null;
    }
    connectionStatus.set("connecting");
  };
}
