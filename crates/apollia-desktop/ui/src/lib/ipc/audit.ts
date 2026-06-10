// Typed IPC wrappers for the audit trail and its hash-chain verification.
//
// Both calls reach the in-process runtime: `getToolAuditTrail` lists tool
// invocations, `verifyAuditRun` checks the tamper-evident journal for a run.
// Keeping them here removes direct `invoke` usage from the `.svelte` files.

import { invoke } from "@tauri-apps/api/core";
import type { AuditTrailEntry } from "$lib/types";

/** Integrity verdict for a run's hash-chained audit journal. */
export interface AuditVerifyResult {
  ok: boolean;
  /** Identifier of the first broken entry, `null` when `ok` is true. */
  broken_at: string | null;
  message: string;
}

/** Reads the latest tool invocations (replaces the direct invoke in the table). */
export async function getToolAuditTrail(limit: number): Promise<AuditTrailEntry[]> {
  return invoke<AuditTrailEntry[]>("get_tool_audit_trail", { limit });
}

/** Verifies the integrity of a run's audit hash chain. */
export async function verifyAuditRun(runId: string): Promise<AuditVerifyResult> {
  return invoke<AuditVerifyResult>("verify_audit_run", { runId });
}
