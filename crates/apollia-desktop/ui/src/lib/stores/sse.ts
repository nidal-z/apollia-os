/**
 * Runtime event store for Apollia Desktop.
 *
 * Listens to Tauri events emitted by the Rust EventBus bridge (`events.rs`)
 * and dispatches to the appropriate Svelte stores.  A watchdog triggers a
 * full IPC refresh if no event is received within 10 seconds.
 *
 * Replaces the previous 3-second polling loop.
 */
import { writable, get } from "svelte/store";
import { t } from "svelte-i18n";
import { invoke } from "@tauri-apps/api/core";
import { listen, emit, type UnlistenFn } from "@tauri-apps/api/event";
import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from "@tauri-apps/plugin-notification";
import type {
  AgentListItem,
  TaskSummary,
  PendingApproval,
  ConnectionStatus,
  LlmBackendConfig,
  TriggerStatus,
  ChatSessionSummary,
} from "$lib/types";
import { onboardingStore } from "./onboarding";
import { refreshSttStatus, refreshTranscriptions } from "./stt";
import {
  appendGlobalToken,
  clearGlobalBuffer,
  addPendingChatApproval,
  removePendingChatApproval,
  addPendingUserInput,
  removePendingUserInput,
  addPendingPlanApproval,
  removePendingPlanApproval,
} from "./chat-global";
import {
  handleThinkingStarted,
  handleThinkingEnded,
  handleDecisionPointRecorded,
} from "./thinking";
import type {
  DecisionPointRecordedEvent,
  ThinkingEndedEvent,
  ThinkingStartedEvent,
} from "$lib/types";

/** Watchdog timeout - triggers a single IPC refresh if no event received. */
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

/**
 * Whether `llmBackends` has been hydrated by at least one successful
 * `list_llm_backends` round-trip. The initial `[]` is otherwise
 * indistinguishable from a confirmed empty list, and consumers that treat
 * "empty" as "no engine available" would announce an absence during startup.
 */
export const llmBackendsHydrated = writable<boolean>(false);

/** List of triggers from the runtime. */
export const triggers = writable<TriggerStatus[]>([]);

/** List of chat sessions from the runtime. */
export const chatSessions = writable<ChatSessionSummary[]>([]);

/**
 * Last inter-agent send announced by the bridge (`null` before the first one).
 *
 * The bridge carries the identities and a payload hash, never the message body
 * on the default path, so this is a signal to re-read rather than a message to
 * render: `AgentMessagesPanel` refetches when the send concerns its agent.
 */
export const lastAgentMessageSent = writable<AgentMessageSent | null>(null);

/** Ticks once per `SharedNamespaceAdded`, so the memory route can re-read. */
export const memoryChanged = writable<number>(0);

/** Real-time session LLM cost - updated on every TokenBudgetUpdated event. */
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
    // runtime not ready yet - keep current state
  }
}

async function refreshTasksViaIpc(): Promise<void> {
  try {
    const result: TaskSummary[] = await invoke("list_tasks", { filter: null });
    tasks.set(result);
  } catch {
    // runtime not ready yet - keep current state
  }
}

async function refreshLlmBackendsViaIpc(): Promise<void> {
  try {
    const result: LlmBackendConfig[] = await invoke("list_llm_backends");
    llmBackends.set(result);
    llmBackendsHydrated.set(true);
  } catch {
    // runtime not ready yet - keep current state, stay un-hydrated
  }
}

// Force an immediate backend-list refresh (used by surfaces that must not wait
// for the next SSE push or the watchdog, e.g. the onboarding chat step).
export async function refreshLlmBackends(): Promise<void> {
  await refreshLlmBackendsViaIpc();
}

async function refreshTriggersViaIpc(): Promise<void> {
  try {
    const result: TriggerStatus[] = await invoke("list_triggers");
    triggers.set(result);
  } catch {
    // runtime not ready yet - keep current state
  }
}

// Force an immediate trigger-list refresh (used after enable/disable so the UI
// reflects the change without waiting for the next SSE push).
export async function refreshTriggers(): Promise<void> {
  await refreshTriggersViaIpc();
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
    // runtime not ready yet - keep current state
  }
}

async function refreshChatSessionsViaIpc(): Promise<void> {
  try {
    const result: ChatSessionSummary[] = await invoke("list_chat_sessions");
    chatSessions.set(result);
  } catch {
    // runtime not ready yet - keep current state
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
      title: get(t)("notifications.native.task_approval_title"),
      body: get(t)("notifications.native.task_approval_body", {
        values: { id: taskId.slice(0, 8) },
      }),
    });
  } catch {
    // Notification API unavailable - silently ignore
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
      title: get(t)("notifications.native.chat_approval_title"),
      body: get(t)("notifications.native.chat_approval_body", {
        values: { tool: toolName, id: sessionId.slice(0, 8) },
      }),
    });
  } catch {
    // Notification API unavailable - silently ignore
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
      title: get(t)("notifications.native.tool_failure_title"),
      body: get(t)("notifications.native.tool_failure_body", {
        values: { tool: toolName, id: sessionId.slice(0, 8) },
      }),
    });
  } catch {
    // Notification API unavailable - silently ignore
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
 * Returns the string value of a field if it is a string, otherwise `fallback`.
 * Guards against accidental `[object Object]` stringification when the
 * payload shape diverges from the expected schema.
 */
function asString(value: unknown, fallback = ""): string {
  return typeof value === "string" ? value : fallback;
}

/**
 * Returns the string value of a field, or `undefined` when the field is
 * missing or not a string.
 */
function asOptionalString(value: unknown): string | undefined {
  return typeof value === "string" ? value : undefined;
}

/**
 * Extracts an inner payload for an externally tagged Rust enum variant.
 * Falls back to the outer payload when the variant key is absent.
 */
function variantPayload(
  payload: Record<string, unknown>,
  variant: string,
): Record<string, unknown> {
  const inner = payload[variant];
  if (inner && typeof inner === "object") {
    return inner as Record<string, unknown>;
  }
  return payload;
}

/** Identities the bridge carries with an `AgentMessageSent`. */
export interface AgentMessageSent {
  /** Sending agent, or `host:<id>` for a host injection. */
  from: string;
  /** Receiving agent. */
  to: string;
  /** Unique identifier of the message. */
  message_id: string;
}

/** Narrow the `AgentMessageSent` payload of a bridge event, field by field. */
function isAgentMessageSent(raw: unknown): raw is AgentMessageSent {
  if (typeof raw !== "object" || raw === null) return false;
  const r = raw as Record<string, unknown>;
  return (
    typeof r.from === "string" &&
    typeof r.to === "string" &&
    typeof r.message_id === "string"
  );
}

/** Handles `ChatApprovalRequired`: enqueues a pending approval + notification. */
function handleChatApprovalRequired(event: TauriRuntimeEvent): void {
  const p = variantPayload(event.payload, "ChatApprovalRequired");
  const sessionId = asOptionalString(p.session_id);
  if (!sessionId) return;
  addPendingChatApproval({
    sessionId,
    messageId: asString(p.message_id),
    toolCallId: asString(p.tool_call_id),
    toolName: asString(p.tool_name),
    inputPreview: asString(p.prompt),
    receivedAt: new Date().toISOString(),
  });
  void sendChatApprovalNotification(sessionId, asString(p.tool_name, "tool"));
}

/** Handles `ChatApprovalResolved` / `ChatApprovalTimeout`: drops the pending row. */
function handleChatApprovalCleared(
  event: TauriRuntimeEvent,
  eventType: "ChatApprovalResolved" | "ChatApprovalTimeout",
): void {
  const p = variantPayload(event.payload, eventType);
  const sessionId = asOptionalString(p.session_id);
  if (!sessionId) return;
  removePendingChatApproval(
    sessionId,
    asOptionalString(p.message_id),
    asOptionalString(p.tool_call_id),
  );
}

/** Handles `ChatUserInputRequired`: enqueues a pending user-input prompt. */
function handleChatUserInputRequired(event: TauriRuntimeEvent): void {
  const p = variantPayload(event.payload, "ChatUserInputRequired");
  addPendingUserInput({
    request_id: asString(p.request_id),
    session_id: asString(p.session_id),
    questions_json: asString(p.questions_json, "[]"),
    context: asOptionalString(p.context) ?? null,
    created_at: new Date().toISOString(),
  });
}

/** Handles `ChatUserInputResolved`: removes the matching pending input. */
function handleChatUserInputResolved(event: TauriRuntimeEvent): void {
  const p = variantPayload(event.payload, "ChatUserInputResolved");
  const requestId = asOptionalString(p.request_id);
  if (requestId) removePendingUserInput(requestId);
}

/** Handles `ChatResponseCompleted`: clears the token buffer + refreshes metrics. */
function handleChatResponseCompleted(event: TauriRuntimeEvent): void {
  const p = variantPayload(event.payload, "ChatResponseCompleted");
  const sessionId = asOptionalString(p.session_id);
  if (!sessionId) return;
  clearGlobalBuffer(sessionId);
  void import("./chatMetrics").then((m) => m.refreshSessionMetrics(sessionId));
}

/** Handles `ChatToolCallCompleted`: surfaces tool failure + refreshes metrics. */
function handleChatToolCallCompleted(event: TauriRuntimeEvent): void {
  const p = variantPayload(event.payload, "ChatToolCallCompleted");
  const sessionId = asOptionalString(p.session_id);
  const toolName = asOptionalString(p.tool_name);
  if (p.success === false && toolName) {
    void sendToolFailureNotification(sessionId ?? "", toolName);
  }
  if (sessionId) {
    void import("./chatMetrics").then((m) => m.refreshSessionMetrics(sessionId));
  }
}

/** Handles `ChatError`: clears the token buffer for the affected session. */
function handleChatError(event: TauriRuntimeEvent): void {
  const p = variantPayload(event.payload, "ChatError");
  const sessionId = asOptionalString(p.session_id);
  if (sessionId) clearGlobalBuffer(sessionId);
}

/** Returns the numeric value of a field if it is a number, otherwise `fallback`. */
function asNumber(value: unknown, fallback = 0): number {
  return typeof value === "number" ? value : fallback;
}

/** Feeds `ThinkingStarted` into the turn-keyed thinking store. */
function handleThinkingStartedEvent(event: TauriRuntimeEvent): void {
  const p = variantPayload(event.payload, "ThinkingStarted");
  const turnId = asOptionalString(p.turn_id);
  if (!turnId) return;
  const payload: ThinkingStartedEvent = {
    turn_id: turnId,
    ts_ms: asNumber(p.ts_ms),
  };
  handleThinkingStarted(payload);
}

/** Feeds `ThinkingEnded` into the turn-keyed thinking store. */
function handleThinkingEndedEvent(event: TauriRuntimeEvent): void {
  const p = variantPayload(event.payload, "ThinkingEnded");
  const turnId = asOptionalString(p.turn_id);
  if (!turnId) return;
  const payload: ThinkingEndedEvent = {
    turn_id: turnId,
    ts_ms: asNumber(p.ts_ms),
    duration_ms: asNumber(p.duration_ms),
    raw_content: asString(p.raw_content),
    tokens: asNumber(p.tokens),
  };
  handleThinkingEnded(payload);
}

/** Feeds `DecisionPointRecorded` into the turn-keyed decision store. */
function handleDecisionPointEvent(event: TauriRuntimeEvent): void {
  const p = variantPayload(event.payload, "DecisionPointRecorded");
  const point = p.point;
  if (!point || typeof point !== "object") return;
  handleDecisionPointRecorded({
    point,
  } as DecisionPointRecordedEvent);
}

/**
 * Dispatches a chat-changed runtime event to the chat-global helpers.
 * Each event type is handled by a dedicated helper to keep this dispatcher's
 * cognitive complexity within the lint budget.
 */
function dispatchChatEvent(event: TauriRuntimeEvent): void {
  const eventType = event.event_type;
  switch (eventType) {
    case "ChatApprovalRequired":
      handleChatApprovalRequired(event);
      return;
    case "ChatApprovalResolved":
    case "ChatApprovalTimeout":
      handleChatApprovalCleared(event, eventType);
      return;
    case "ChatUserInputRequired":
      handleChatUserInputRequired(event);
      return;
    case "ChatUserInputResolved":
      handleChatUserInputResolved(event);
      return;
    case "ChatResponseCompleted":
      handleChatResponseCompleted(event);
      return;
    case "ChatToolCallCompleted":
      handleChatToolCallCompleted(event);
      return;
    case "ChatError":
      handleChatError(event);
      return;
    case "ThinkingStarted":
      handleThinkingStartedEvent(event);
      return;
    case "ThinkingEnded":
      handleThinkingEndedEvent(event);
      return;
    case "DecisionPointRecorded":
      handleDecisionPointEvent(event);
      return;
    default:
      return;
  }
}

/** Handles `PlanSubmitted`: enqueues the plan awaiting approval for the inbox. */
function handlePlanSubmitted(event: TauriRuntimeEvent): void {
  const p = variantPayload(event.payload, "PlanSubmitted");
  const sessionId = asOptionalString(p.session_id);
  if (!sessionId) return;
  const plan =
    p.plan && typeof p.plan === "object"
      ? (p.plan as Record<string, unknown>)
      : {};
  const steps = Array.isArray(plan.steps) ? plan.steps : [];
  const firstTitle =
    steps.length > 0 &&
    typeof (steps[0] as Record<string, unknown>).title === "string"
      ? ((steps[0] as Record<string, unknown>).title as string)
      : "";
  addPendingPlanApproval({
    sessionId,
    planId: asString(plan.plan_id),
    stepCount: steps.length,
    summary: firstTitle || `${steps.length} step(s)`,
    submittedAt: new Date().toISOString(),
  });
}

/**
 * Dispatches a session-keyed plan-mode event to the global plan-approval store
 * so the inbox tracks plans awaiting approval across sessions.
 */
function dispatchPlanModeEvent(event: TauriRuntimeEvent): void {
  const inner = variantPayload(event.payload, event.event_type);
  const sessionId = asOptionalString(inner.session_id);
  switch (event.event_type) {
    case "PlanSubmitted":
      handlePlanSubmitted(event);
      return;
    case "ChatPlanApproved":
    case "ChatPlanRejected":
      if (sessionId) removePendingPlanApproval(sessionId);
      return;
    case "ChatPlanPhaseChanged": {
      const phase = asOptionalString(inner.phase);
      if (sessionId && phase && phase !== "awaiting_approval") {
        removePendingPlanApproval(sessionId);
      }
      return;
    }
    default:
      return;
  }
}

/**
 * Dispatches a runtime event to the appropriate store by refreshing the
 * relevant domain via IPC.  This ensures data consistency (the IPC command
 * returns the full current state, not just a delta).
 */
function dispatchEvent(event: TauriRuntimeEvent): void {
  switch (event.category) {
    case "agent-changed": {
      if (event.event_type === "AgentMessageSent") {
        const sent = variantPayload(event.payload, "AgentMessageSent");
        if (isAgentMessageSent(sent)) {
          lastAgentMessageSent.set(sent);
        }
      }
      void refreshAgentsViaIpc();
      break;
    }
    case "task-changed":
      void refreshTasksViaIpc();
      break;
    case "approval-changed":
      void refreshPendingApprovalsViaIpc();
      void refreshTasksViaIpc();
      break;
    case "llm-changed":
      if (event.event_type === "TokenBudgetUpdated") {
        // Direct store update - no IPC round-trip needed, payload carries all fields.
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
    case "chat-changed":
      void refreshChatSessionsViaIpc();
      dispatchChatEvent(event);
      break;
    case "plan-mode":
      dispatchPlanModeEvent(event);
      break;
    case "session-metrics":
      void import("./chatMetrics").then((m) => m.refreshActiveSessionMetrics());
      break;
    case "memory-changed":
      memoryChanged.update((n) => n + 1);
      break;
    case "stt-changed":
      void refreshSttStatus();
      void refreshTranscriptions();
      break;
    case "onboarding-changed":
      // Keeps the legacy store in sync with backend phases. The agent-driven
      // modal listens directly to the "runtime-event" channel for
      // OnboardingRequired / OnboardingCompleted variants in App.svelte.
      onboardingStore.setRequired();
      break;
    case "system":
      // ContextCompacted carries no session_id; refresh the session the user
      // is looking at so the context gauge tracks the compaction instead of
      // waiting for the next turn-completion refresh.
      if (event.event_type === "ContextCompacted") {
        void import("./chatMetrics").then((m) => m.refreshActiveSessionMetrics());
        break;
      }
      // AllReady / ShutdownRequested and the rest - refresh everything
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

  // 2b. Global chat-token listener - accumulates tokens into the global buffer
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
