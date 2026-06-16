// Typed IPC wrappers for the plan-mode approval gate.
//
// Mirrors the real runtime payload: the engine emits a `PlanApprovalRequired`
// event carrying the generated plan, and the operator resolves the gate by
// run_id through the `submit_plan_decision` Tauri command.

import { invoke } from "@tauri-apps/api/core";

/** A single step of a proposed plan, as carried by the runtime event. */
export interface PlanStep {
  step_id: string;
  description: string;
  depends_on: string[];
  tool_hint?: string | null;
  model_hint?: string | null;
}

/** Plan awaiting an operator decision, extracted from `PlanApprovalRequired`. */
export interface ProposedPlan {
  run_id: string;
  plan_id: string;
  task_id: string;
  step_count: number;
  steps: PlanStep[];
  ttl_secs: number;
}

/**
 * Operator decision sent back to the gate.
 *
 * Internally tagged on `decision` so it deserializes into the Rust
 * `PlanDecisionDto`. The `edit` variant is wired by the edit flow.
 */
export type PlanDecision =
  | { decision: "approve" }
  | { decision: "reject"; reason?: string }
  | { decision: "edit"; revised_steps: PlanStep[] };

/**
 * Submits the operator's decision for the plan pending approval on `runId`.
 *
 * Resolves to `void` on success; rejects with the runtime error string when no
 * gate is pending for `runId` or the registry is unavailable.
 */
export async function submitPlanDecision(
  runId: string,
  decision: PlanDecision,
): Promise<void> {
  return invoke<void>("submit_plan_decision", { runId, decision });
}

// ── Conversational (session-keyed) plan mode ──────────────────────────────────
//
// Distinct from the run-keyed ORIA gate above: these wrappers target the
// chat-native soft gate, identified by `sessionId`. They back the header toggle
// chip (`setPlanMode`) and the in-conversation review card (`approvePlan` /
// `rejectPlan`).

/**
 * Reads the runtime-level default plan-mode state for new chat sessions.
 *
 * The value comes from the `[chat] plan_mode_default` key of `apollia.toml`,
 * read once at boot. The desktop uses it to seed its per-user preference, so the
 * config stays the single source of truth for the default.
 *
 * Resolves to the boolean default; rejects with the runtime error string when
 * the runtime handle is unavailable.
 */
export async function getPlanModeDefault(): Promise<boolean> {
  return invoke<boolean>("get_plan_mode_default");
}

/**
 * Enables or disables plan mode for a chat session.
 *
 * Resolves to `void` on success; rejects with the runtime error string when the
 * chat subsystem is unavailable or the session does not exist.
 */
export async function setPlanMode(
  sessionId: string,
  enabled: boolean,
): Promise<void> {
  return invoke<void>("set_plan_mode", { sessionId, enabled });
}

/** Snapshot returned by `get_chat_plan`: the plan plus the session phase. */
export interface ChatPlanSnapshot {
  /** Current plan payload (the unified `apollia_core::plan::Plan`) or null. */
  plan: unknown;
  /** Session plan-mode phase (the authoritative gate state). */
  phase: string;
}

/**
 * Reads the current plan snapshot for a session.
 *
 * Lets the plan tab hydrate on mount from authoritative runtime state instead of
 * relying solely on live events, so a plan produced before the tab was opened
 * still renders. The `phase` is the gate state: an executed plan keeps its
 * `awaiting_approval` status on the plan object, but its session phase has moved
 * on, so the caller decides the approval card from `phase`, not the plan status.
 */
export async function getChatPlan(sessionId: string): Promise<ChatPlanSnapshot> {
  return invoke<ChatPlanSnapshot>("get_chat_plan", { sessionId });
}

/**
 * Approves the submitted plan for a session (soft conversational gate).
 *
 * Resolves to `void` on success; rejects with the runtime error string when the
 * session is no longer awaiting approval or the chat subsystem is unavailable.
 */
export async function approvePlan(sessionId: string): Promise<void> {
  return invoke<void>("approve_plan", { sessionId });
}

/**
 * Rejects the submitted plan for a session with an optional reason.
 *
 * Resolves to `void` on success; rejects with the runtime error string when the
 * session is no longer awaiting approval or the chat subsystem is unavailable.
 */
export async function rejectPlan(
  sessionId: string,
  reason?: string,
): Promise<void> {
  return invoke<void>("reject_plan", { sessionId, reason });
}
