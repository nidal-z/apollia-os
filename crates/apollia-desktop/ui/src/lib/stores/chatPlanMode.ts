// Session-keyed conversational plan-mode store.
//
// Distinct from `planMode.ts`, which tracks the run-keyed ORIA approval gate
// (category "plan-approval"). This store tracks the chat-native soft gate
// (category "plan-mode", STORY-603 events), scoped to a single chat session.
//
// The runtime bridge re-emits every `RuntimeEvent` as a `runtime-event` Tauri
// event with `{ category, event_type, payload }`, where `payload` is the
// externally tagged enum (`{ PlanSubmitted: { session_id, plan } }`). This
// store subscribes to that channel, ignores any event for another session, and
// projects the gate events onto a small phase machine. Components read this
// store and submit decisions through `$lib/ipc/planMode`; they never call
// `invoke` directly.

import { writable } from "svelte/store";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { PlanPhase } from "$lib/types";

/** A single step of a session plan (the unified `apollia_core::plan::PlanStep`). */
export interface SessionPlanStep {
  step_id: string;
  title: string;
  description: string;
  status: string;
  depends_on: string[];
  tool_hint: string | null;
  model_hint: string | null;
  rationale: string | null;
  provenance: SessionStepProvenance;
}

/** Provenance of a plan step (origin + reason + timestamp). */
export interface SessionStepProvenance {
  origin: string;
  reason: string | null;
  at: number;
}

/** A session plan (the unified `apollia_core::plan::Plan`). */
export interface SessionPlan {
  plan_id: string;
  revision: number;
  status: string;
  steps: SessionPlanStep[];
}

/** Last mutation applied to the session plan (kind + concerned step). */
export interface SessionPlanMutation {
  kind: string;
  step_id: string | null;
  reason: string | null;
}

/** Lifecycle status of the session review card. */
export type ChatPlanStatus =
  | "idle"
  | "pending_approval"
  | "approved"
  | "rejected"
  | "error";

/** Shared state of the conversational plan gate for the tracked session. */
export interface ChatPlanState {
  status: ChatPlanStatus;
  /** Chat session currently tracked, `null` when no listener is active. */
  sessionId: string | null;
  /** Current plan-mode phase of the session. */
  phase: PlanPhase;
  /** Latest plan for the session (`null` until a plan is produced). */
  plan: SessionPlan | null;
  /** Last mutation carried by a `PlanUpdated` event (`null` until one lands). */
  lastMutation: SessionPlanMutation | null;
  errorMessage: string | null;
}

function initialState(): ChatPlanState {
  return {
    status: "idle",
    sessionId: null,
    phase: "done",
    plan: null,
    lastMutation: null,
    errorMessage: null,
  };
}

function toMutation(value: unknown): SessionPlanMutation | null {
  if (!value || typeof value !== "object") return null;
  const m = value as Record<string, unknown>;
  if (typeof m.kind !== "string") return null;
  return {
    kind: m.kind,
    step_id: asOptionalString(m.step_id),
    reason: asOptionalString(m.reason),
  };
}

export const chatPlanState = writable<ChatPlanState>(initialState());

/** Shape of the bridge envelope (`TauriRuntimeEvent`). */
interface TauriRuntimeEvent {
  category: string;
  event_type: string;
  payload: Record<string, unknown>;
}

/** Extracts the inner object of an externally tagged enum variant. */
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

function asString(value: unknown, fallback = ""): string {
  return typeof value === "string" ? value : fallback;
}

function asOptionalString(value: unknown): string | null {
  return typeof value === "string" ? value : null;
}

function toProvenance(value: unknown): SessionStepProvenance {
  const p = (value && typeof value === "object" ? value : {}) as Record<
    string,
    unknown
  >;
  return {
    origin: asString(p.origin, "initial"),
    reason: asOptionalString(p.reason),
    at: typeof p.at === "number" ? p.at : 0,
  };
}

function toPlanStep(value: unknown): SessionPlanStep {
  const s = (value && typeof value === "object" ? value : {}) as Record<
    string,
    unknown
  >;
  return {
    step_id: asString(s.step_id),
    title: asString(s.title),
    description: asString(s.description),
    status: asString(s.status, "pending"),
    depends_on: Array.isArray(s.depends_on)
      ? s.depends_on.map((d) => asString(d))
      : [],
    tool_hint: asOptionalString(s.tool_hint),
    model_hint: asOptionalString(s.model_hint),
    rationale: asOptionalString(s.rationale),
    provenance: toProvenance(s.provenance),
  };
}

function toSessionPlan(value: unknown): SessionPlan | null {
  if (!value || typeof value !== "object") return null;
  const p = value as Record<string, unknown>;
  const rawSteps = Array.isArray(p.steps) ? p.steps : [];
  return {
    plan_id: asString(p.plan_id),
    revision: typeof p.revision === "number" ? p.revision : 0,
    status: asString(p.status, "draft"),
    steps: rawSteps.map(toPlanStep),
  };
}

const KNOWN_PHASES: readonly PlanPhase[] = [
  "discovery",
  "drafting",
  "awaiting_approval",
  "executing",
  "done",
];

function toPhase(value: unknown, fallback: PlanPhase): PlanPhase {
  return KNOWN_PHASES.includes(value as PlanPhase)
    ? (value as PlanPhase)
    : fallback;
}

/**
 * Projects one bridge envelope onto the store for `activeSessionId`.
 *
 * Events for any other session are ignored (AC-1). Exported for unit tests.
 */
export function dispatchChatPlan(
  event: TauriRuntimeEvent,
  activeSessionId: string,
): void {
  if (event.category !== "plan-mode") return;

  const inner = variantPayload(event.payload, event.event_type);
  if (asString(inner.session_id) !== activeSessionId) return;

  switch (event.event_type) {
    case "PlanUpdated": {
      const plan = toSessionPlan(inner.plan);
      const mutation = toMutation(inner.mutation);
      chatPlanState.update((s) => ({
        ...s,
        sessionId: activeSessionId,
        plan: plan ?? s.plan,
        lastMutation: mutation ?? s.lastMutation,
      }));
      break;
    }
    case "PlanSubmitted": {
      const plan = toSessionPlan(inner.plan);
      chatPlanState.update((s) => ({
        ...s,
        status: "pending_approval",
        sessionId: activeSessionId,
        phase: "awaiting_approval",
        plan: plan ?? s.plan,
        errorMessage: null,
      }));
      break;
    }
    case "ChatPlanApproved":
      chatPlanState.update((s) => ({
        ...s,
        status: "approved",
        phase: "executing",
      }));
      break;
    case "ChatPlanRejected":
      chatPlanState.update((s) => ({ ...s, status: "rejected" }));
      break;
    case "ChatPlanPhaseChanged": {
      const phase = toPhase(inner.phase, "done");
      chatPlanState.update((s) => ({
        ...s,
        sessionId: activeSessionId,
        phase,
        // Keep the awaiting card up only while the phase says so.
        status:
          phase === "awaiting_approval"
            ? "pending_approval"
            : s.status === "pending_approval"
              ? "idle"
              : s.status,
      }));
      break;
    }
    default:
      break;
  }
}

/**
 * Subscribes to the runtime bridge for one chat session's plan-mode events.
 *
 * Resets the store to the tracked session, then projects every matching
 * "plan-mode" envelope onto it. Returns an unlisten function; call it from a
 * component `$effect` cleanup so no orphaned handler survives the component.
 */
export async function startChatPlanListener(
  activeSessionId: string,
): Promise<() => void> {
  chatPlanState.set({ ...initialState(), sessionId: activeSessionId });
  const unlisten: UnlistenFn = await listen<TauriRuntimeEvent>(
    "runtime-event",
    (event) => dispatchChatPlan(event.payload, activeSessionId),
  );
  return unlisten;
}

/** Records a successful approval (optimistic, before the event confirms it). */
export function setChatPlanApproved(): void {
  chatPlanState.update((s) => ({
    ...s,
    status: "approved",
    phase: "executing",
  }));
}

/** Records a successful rejection (optimistic, before the event confirms it). */
export function setChatPlanRejected(): void {
  chatPlanState.update((s) => ({ ...s, status: "rejected" }));
}

/** Records a failed decision: the gate stays open, the error is surfaced. */
export function setChatPlanError(message: string): void {
  chatPlanState.update((s) => ({
    ...s,
    status: "error",
    errorMessage: message,
  }));
}

/** Resets the store to its idle state (test helper + session teardown). */
export function resetChatPlan(): void {
  chatPlanState.set(initialState());
}
