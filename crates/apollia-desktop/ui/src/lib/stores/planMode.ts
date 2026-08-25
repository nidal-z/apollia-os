// Plan-mode store: single source of truth for the proposed plan and its
// approval lifecycle.
//
// The runtime bridge re-emits every `RuntimeEvent` as a `runtime-event` Tauri
// event with `{ category, event_type, payload }`, where `payload` is the
// externally tagged enum (`{ PlanApprovalRequired: { ... } }`). This store
// subscribes to that channel, projects the gate events onto a small state
// machine, and exposes typed setters. Components never call `invoke` directly:
// they read this store and submit decisions through `$lib/ipc/planMode`.

import { writable, get } from "svelte/store";
import { t } from "svelte-i18n";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { ProposedPlan, PlanStep } from "$lib/ipc/planMode";

/** Lifecycle of the plan awaiting an operator decision. */
export type PlanModeStatus =
  | "idle"
  | "pending_approval"
  | "approved"
  | "rejected"
  | "editing"
  | "error";

/** Shared plan-mode state consumed by the review and edit components. */
export interface PlanModeState {
  status: PlanModeStatus;
  /** Run identifier of the gate currently tracked, `null` when idle. */
  runId: string | null;
  /** Plan awaiting a decision. */
  plan: ProposedPlan | null;
  /** Previous plan, kept across a replan so the Builder can render a diff. */
  previousPlan: ProposedPlan | null;
  errorMessage: string | null;
}

const initial: PlanModeState = {
  status: "idle",
  runId: null,
  plan: null,
  previousPlan: null,
  errorMessage: null,
};

export const planModeState = writable<PlanModeState>(initial);

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

function toProposedPlan(p: Record<string, unknown>): ProposedPlan {
  const rawSteps = Array.isArray(p.steps) ? p.steps : [];
  const steps: PlanStep[] = rawSteps.map((s) => {
    const step = s as Record<string, unknown>;
    return {
      step_id: asString(step.step_id),
      description: asString(step.description),
      depends_on: Array.isArray(step.depends_on)
        ? step.depends_on.map((d) => asString(d))
        : [],
      tool_hint: typeof step.tool_hint === "string" ? step.tool_hint : null,
      model_hint: typeof step.model_hint === "string" ? step.model_hint : null,
    };
  });
  return {
    run_id: asString(p.run_id),
    plan_id: asString(p.plan_id),
    task_id: asString(p.task_id),
    step_count: typeof p.step_count === "number" ? p.step_count : steps.length,
    steps,
    ttl_secs: typeof p.ttl_secs === "number" ? p.ttl_secs : 0,
  };
}

function handlePlanApprovalRequired(payload: Record<string, unknown>): void {
  const plan = toProposedPlan(variantPayload(payload, "PlanApprovalRequired"));
  if (!plan.run_id) return;
  planModeState.update((s) => ({
    status: "pending_approval",
    runId: plan.run_id,
    plan,
    // Keep the prior plan of the same run so a replan can be diffed.
    previousPlan:
      s.plan && s.plan.run_id === plan.run_id ? s.plan : s.previousPlan,
    errorMessage: null,
  }));
}

function runIdMatches(
  current: string | null,
  payload: Record<string, unknown>,
  variant: string,
): boolean {
  const p = variantPayload(payload, variant);
  return current !== null && asString(p.run_id) === current;
}

function dispatch(event: TauriRuntimeEvent): void {
  if (event.category !== "plan-approval") return;
  switch (event.event_type) {
    case "PlanApprovalRequired":
      handlePlanApprovalRequired(event.payload);
      break;
    case "PlanApproved":
      planModeState.update((s) =>
        runIdMatches(s.runId, event.payload, "PlanApproved")
          ? { ...s, status: "approved" }
          : s,
      );
      break;
    case "PlanRejected":
      planModeState.update((s) =>
        runIdMatches(s.runId, event.payload, "PlanRejected")
          ? { ...s, status: "rejected" }
          : s,
      );
      break;
    case "PlanAbandoned":
      planModeState.update((s) =>
        runIdMatches(s.runId, event.payload, "PlanAbandoned")
          ? {
              ...s,
              status: "error",
              errorMessage: asString(
                variantPayload(event.payload, "PlanAbandoned").reason,
                get(t)("chat.plan_abandoned_reason"),
              ),
            }
          : s,
      );
      break;
    default:
      break;
  }
}

/**
 * Subscribes to the runtime bridge and projects plan-gate events onto the
 * store. Returns an unlisten function; call it from a component `$effect`
 * cleanup so no orphaned handler survives the component.
 */
export async function startPlanModeListener(): Promise<() => void> {
  const unlisten: UnlistenFn = await listen<TauriRuntimeEvent>(
    "runtime-event",
    (event) => dispatch(event.payload),
  );
  return unlisten;
}

export function setPlanEditing(): void {
  planModeState.update((s) => ({ ...s, status: "editing" }));
}

export function setPlanApproved(): void {
  planModeState.update((s) => ({ ...s, status: "approved" }));
}

export function setPlanRejected(): void {
  planModeState.update((s) => ({ ...s, status: "rejected" }));
}

export function setPlanError(message: string): void {
  planModeState.update((s) => ({
    ...s,
    status: "error",
    errorMessage: message,
  }));
}

export function resetPlanMode(): void {
  planModeState.set(initial);
}

// Exported for unit tests: drives the store from a raw bridge envelope.
export const __test = { dispatch };
