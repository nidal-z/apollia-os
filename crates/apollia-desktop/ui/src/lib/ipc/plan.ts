// Typed contract for the unified plan (mirrors `apollia_core::plan`), plus the
// read-only IPC wrapper for the plan mutation history.
//
// The conversational plan path (chat-native, category "plan-mode") streams the
// plan onto the session-scoped store in `$lib/stores/chatPlanMode`. That store
// owns the single `runtime-event` listener filtered by session. This module is
// the strict type contract consumed by the DAG panel and the step nodes; it
// does NOT open a second listener on the same channel.
//
// Field shapes follow the real wire format produced by serde on the Rust side:
//   - `StepStatus` is serialized snake_case (`pending`, `in_progress`, ...).
//   - `provenance.at` / `mutation.at` are Unix timestamps in seconds (i64),
//     not ISO-8601 strings.
//   - `description` is a plain string on the wire; we keep it non-null and
//     tolerate an empty string for steps that carry no detail.

import { invoke } from "@tauri-apps/api/core";

/** Scope of a plan: a chat session or an ORIA task. */
export type PlanScope =
  | { Chat: { session_id: string } }
  | { Task: { task_id: string } };

/** Life-cycle status of a plan. */
export type PlanStatus =
  | "draft"
  | "submitted"
  | "approved"
  | "rejected"
  | "executing"
  | "completed"
  | "abandoned";

/** Execution status of a single step (snake_case wire form). */
export type StepStatus =
  | "pending"
  | "in_progress"
  | "completed"
  | "skipped"
  | "failed";

const STEP_STATUSES: readonly StepStatus[] = [
  "pending",
  "in_progress",
  "completed",
  "skipped",
  "failed",
];

/**
 * Narrows an arbitrary string (as carried by the tolerant session store) to a
 * strict {@link StepStatus}. Unknown values fall back to `"pending"` so the DAG
 * never renders an undefined status.
 */
export function toStepStatus(value: string): StepStatus {
  return (STEP_STATUSES as readonly string[]).includes(value)
    ? (value as StepStatus)
    : "pending";
}

/** Origin of a step (snake_case wire form, `replan` carries a revision). */
export type StepOrigin =
  | "initial"
  | { replan: number }
  | "user_inject"
  | "agent_edit";

export interface StepProvenance {
  origin: StepOrigin;
  reason: string | null;
  /** Unix timestamp in seconds. */
  at: number;
}

export interface PlanStep {
  step_id: string;
  title: string;
  description: string;
  status: StepStatus;
  depends_on: string[];
  tool_hint: string | null;
  model_hint: string | null;
  rationale: string | null;
  provenance: StepProvenance;
}

export interface Plan {
  plan_id: string;
  scope?: PlanScope;
  revision: number;
  status: string;
  steps: PlanStep[];
}

/** Nature of a mutation applied to a plan (snake_case wire form). */
export type PlanMutationKind =
  | "propose"
  | "add_step"
  | "modify_step"
  | "remove_step"
  | "reorder"
  | "status_change"
  | "submit"
  | "approve"
  | "reject";

export interface PlanMutation {
  kind: PlanMutationKind;
  step_id: string | null;
  reason: string | null;
  before: PlanStep | null;
  after: PlanStep | null;
  /** Unix timestamp in seconds. */
  at: number;
}

/** Externally tagged plan-mode events as serialized by the runtime bridge. */
export type PlanModePayload =
  | { PlanUpdated: { session_id: string; plan: Plan; mutation: PlanMutation } }
  | { PlanSubmitted: { session_id: string; plan: Plan } }
  | { ChatPlanApproved: { session_id: string } }
  | { ChatPlanRejected: { session_id: string; reason: string | null } };

/** Tauri envelope on the "runtime-event" channel. */
export interface RuntimeEventEnvelope {
  category: string;
  event_type: string;
  payload: unknown;
}

/** Category of the chat-native plan stream on the `runtime-event` channel. */
export const PLAN_MODE_CATEGORY = "plan-mode";

/**
 * Returns the ordered plan mutation history for `sessionId`.
 *
 * Mutations come back in insertion order (oldest first), including removal
 * tombstones, so the history scrubber can replay the plan construction revision
 * by revision. A known session with no recorded mutations resolves to an empty
 * array; the IPC rejects when the chat subsystem is unavailable or the session
 * does not exist.
 */
export async function getPlanMutations(
  sessionId: string,
): Promise<PlanMutation[]> {
  return invoke<PlanMutation[]>("list_plan_mutations", { sessionId });
}
