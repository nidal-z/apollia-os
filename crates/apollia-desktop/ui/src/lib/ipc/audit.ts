// Typed IPC wrapper for the audit trail: `getToolAuditTrail` lists tool
// invocations. Keeping it here removes direct `invoke` usage from the
// `.svelte` files.

import { invoke } from "@tauri-apps/api/core";
import type { AuditTrailEntry } from "$lib/types";
import type { PlanMutationKind, PlanStep } from "$lib/ipc/plan";

/**
 * Audit entry representing a single plan mutation, mirroring the runtime
 * `PlanMutationSnapshot`. The single-step `before` / `after` delta is lossy for
 * a `propose` mutation (which builds a whole plan in one move); the viewer falls
 * back to the `after` state for that case.
 *
 * `type` is a viewer-side discriminator (not present on the wire snapshot) so a
 * mixed feed of tool and plan entries can be narrowed without a runtime probe.
 */
export interface PlanMutationEntry {
  type: "plan_mutation";
  /** Stable run identifier this mutation belongs to. */
  run_id: string;
  /** Chat session that owns the plan. */
  session_id: string;
  /** 0-based ordinal within the run (strictly increasing, contiguous). */
  ordinal: number;
  /** Nature of the mutation. */
  kind: PlanMutationKind;
  /** Affected step, `null` for whole-plan mutations. */
  step_id: string | null;
  /** Human-readable reason recorded by the agent or user, if any. */
  reason: string | null;
  /** State of the affected step before the mutation, `null` for an addition. */
  before: PlanStep | null;
  /** State of the affected step after the mutation, `null` for a removal. */
  after: PlanStep | null;
  /** Plan revision after the mutation was applied. */
  revision: number;
  /** RFC3339 UTC timestamp of the entry. */
  ts: string;
}

/** A displayable audit row: a tool invocation or a plan mutation. */
export type AuditRow =
  | { type: "tool"; entry: AuditTrailEntry }
  | { type: "plan_mutation"; entry: PlanMutationEntry };

/** Reads the latest tool invocations (replaces the direct invoke in the table). */
export async function getToolAuditTrail(limit: number): Promise<AuditTrailEntry[]> {
  return invoke<AuditTrailEntry[]>("get_tool_audit_trail", { limit });
}
