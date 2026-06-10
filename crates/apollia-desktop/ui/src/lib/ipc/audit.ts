// Typed IPC wrappers for the audit trail and its hash-chain verification.
//
// Both calls reach the in-process runtime: `getToolAuditTrail` lists tool
// invocations, `verifyAuditRun` checks the tamper-evident journal for a run.
// Keeping them here removes direct `invoke` usage from the `.svelte` files.

import { invoke } from "@tauri-apps/api/core";
import type { AuditTrailEntry } from "$lib/types";
import type { PlanMutationKind, PlanStep } from "$lib/ipc/plan";

/** Integrity verdict for a run's hash-chained audit journal. */
export interface AuditVerifyResult {
  ok: boolean;
  /** Identifier of the first broken entry, `null` when `ok` is true. */
  broken_at: string | null;
  message: string;
}

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

/** Verifies the integrity of a run's audit hash chain. */
export async function verifyAuditRun(runId: string): Promise<AuditVerifyResult> {
  return invoke<AuditVerifyResult>("verify_audit_run", { runId });
}
