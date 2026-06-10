// Pure mapping helpers for the audit trail viewer.
//
// Turns a raw audit entry into a typed, displayable row and computes the compact
// before/after field diff for a plan mutation. Kept DOM-free and IPC-free so the
// logic is unit-testable in isolation; the row component only renders the result.

import type { AuditRow, PlanMutationEntry } from "$lib/ipc/audit";
import type { AuditTrailEntry } from "$lib/types";
import type { PlanMutationKind, PlanStep } from "$lib/ipc/plan";

/** A single field changed between two states of a step, for the mini-diff. */
export interface PlanFieldDiff {
  field: "title" | "description" | "status" | "depends_on" | "tool_hint" | "model_hint";
  status: "added" | "modified" | "removed";
  before: string | null;
  after: string | null;
}

/** Fields rendered by the mini-diff, in display order. */
const DIFF_FIELDS: readonly PlanFieldDiff["field"][] = [
  "title",
  "description",
  "status",
  "depends_on",
  "tool_hint",
  "model_hint",
];

/** Reads a step field as a normalized string, `null` when empty or absent. */
function fieldValue(step: PlanStep, field: PlanFieldDiff["field"]): string | null {
  switch (field) {
    case "title":
      return step.title.length > 0 ? step.title : null;
    case "description":
      return step.description.length > 0 ? step.description : null;
    case "status":
      return step.status;
    case "depends_on":
      return step.depends_on.length > 0 ? step.depends_on.join(", ") : null;
    case "tool_hint":
      return step.tool_hint;
    case "model_hint":
      return step.model_hint;
  }
}

/**
 * Narrows a raw audit entry to a plan mutation entry, returning `null` when the
 * shape does not match. Only the discriminator and the structural fields needed
 * for rendering are checked; the runtime is the authority on the full payload.
 */
function asPlanMutationEntry(raw: unknown): PlanMutationEntry | null {
  if (typeof raw !== "object" || raw === null) return null;
  const candidate = raw as Record<string, unknown>;
  if (candidate.type !== "plan_mutation") return null;
  if (typeof candidate.kind !== "string") return null;
  if (typeof candidate.ordinal !== "number") return null;
  return raw as PlanMutationEntry;
}

/**
 * Maps a raw audit entry to a displayable row.
 *
 * Entries tagged `plan_mutation` become a `plan_mutation` row; everything else
 * falls back to a `tool` row, preserving the existing tool feed unchanged. Pure:
 * no DOM, no IPC.
 */
export function mapAuditRow(raw: unknown): AuditRow {
  const planEntry = asPlanMutationEntry(raw);
  if (planEntry !== null) {
    return { type: "plan_mutation", entry: planEntry };
  }
  return { type: "tool", entry: raw as AuditTrailEntry };
}

/**
 * Computes the compact field diff between the `before` and `after` states of a
 * plan mutation.
 *
 * - `before` null, `after` set: every present field is `added`.
 * - `before` set, `after` null: every present field is `removed`.
 * - both set: only the fields that changed are `modified`.
 * - both null (whole-plan mutation, e.g. submit): empty diff.
 *
 * Unchanged fields are dropped to keep the row compact.
 */
export function computePlanFieldDiff(entry: PlanMutationEntry): PlanFieldDiff[] {
  const { before, after } = entry;

  if (before === null && after === null) return [];

  if (before === null && after !== null) {
    const result: PlanFieldDiff[] = [];
    for (const field of DIFF_FIELDS) {
      const value = fieldValue(after, field);
      if (value !== null) {
        result.push({ field, status: "added", before: null, after: value });
      }
    }
    return result;
  }

  if (before !== null && after === null) {
    const result: PlanFieldDiff[] = [];
    for (const field of DIFF_FIELDS) {
      const value = fieldValue(before, field);
      if (value !== null) {
        result.push({ field, status: "removed", before: value, after: null });
      }
    }
    return result;
  }

  const result: PlanFieldDiff[] = [];
  for (const field of DIFF_FIELDS) {
    const prev = fieldValue(before as PlanStep, field);
    const next = fieldValue(after as PlanStep, field);
    if (prev !== next) {
      result.push({ field, status: "modified", before: prev, after: next });
    }
  }
  return result;
}

/** Lucide icon key associated with a plan mutation kind. */
export function planMutationIconKey(kind: PlanMutationKind): string {
  switch (kind) {
    case "propose":
      return "list-plus";
    case "add_step":
      return "plus-circle";
    case "modify_step":
      return "pencil";
    case "remove_step":
      return "trash-2";
    case "reorder":
      return "arrow-up-down";
    case "status_change":
      return "circle-dot";
    case "submit":
      return "send";
    case "approve":
      return "check-circle-2";
    case "reject":
      return "x-circle";
  }
}
