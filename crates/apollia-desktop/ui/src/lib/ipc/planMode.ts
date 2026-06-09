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
