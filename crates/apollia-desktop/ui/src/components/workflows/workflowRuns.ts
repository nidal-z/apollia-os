/**
 * The runs of a workflow, as decisions: how a run and a step are named and
 * toned, which runs are still going, how far a run is, how long it took, how
 * its inputs read on one line, and what a workflow may do according to the
 * package that declares it. No i18n, no IPC.
 */
import type { WorkflowRun, WorkflowStep } from "$lib/ipc/workflows";

/** A badge tone, among those `Badge` knows. */
export type Tone = "success" | "danger" | "warning" | "info" | "neutral";

/** Translation key of the outcome of a run. */
export function runLabelKey(status: string): string {
  switch (status) {
    case "running":
      return "workflows.run_running";
    case "completed":
      return "workflows.run_completed";
    case "failed":
      return "workflows.run_failed";
    case "canceled":
      return "workflows.run_canceled";
    default:
      return "workflows.run_unknown";
  }
}

/** The tone of the outcome of a run. */
export function runTone(status: string): Tone {
  switch (status) {
    case "running":
      return "info";
    case "completed":
      return "success";
    case "failed":
      return "danger";
    case "canceled":
      return "warning";
    default:
      return "neutral";
  }
}

/** Translation key of the state of a step. */
export function stepLabelKey(status: string): string {
  switch (status) {
    case "pending":
      return "workflows.step_pending";
    case "running":
      return "workflows.step_running";
    case "completed":
      return "workflows.step_completed";
    case "failed":
      return "workflows.step_failed";
    case "canceled":
      return "workflows.step_canceled";
    case "not_started":
      return "workflows.step_not_started";
    default:
      return "workflows.run_unknown";
  }
}

/** The tone of the state of a step. */
export function stepTone(status: string): Tone {
  if (status === "pending" || status === "not_started") return "neutral";
  return runTone(status);
}

/** Whether `run` is still going. */
export function isRunning(run: Pick<WorkflowRun, "status">): boolean {
  return run.status === "running";
}

/** The runs still going, in the order given. */
export function runningRuns(runs: WorkflowRun[]): WorkflowRun[] {
  return runs.filter(isRunning);
}

/** The most recent run, the list being newest first. */
export function lastRun(runs: WorkflowRun[]): WorkflowRun | null {
  return runs[0] ?? null;
}

/**
 * The runs a history shows: the first `limit`, or all of them once the user
 * asked to see everything.
 */
export function shownRuns(runs: WorkflowRun[], all: boolean, limit: number): WorkflowRun[] {
  return all ? runs : runs.slice(0, limit);
}

/**
 * How long a span lasted, in milliseconds: to its end, or to `now` while it
 * is still going. `null` when it never started or a date cannot be read.
 */
export function elapsedMs(
  startedAt: string | null,
  endedAt: string | null,
  now: number,
): number | null {
  if (!startedAt) return null;
  const start = Date.parse(startedAt);
  if (Number.isNaN(start)) return null;
  const end = endedAt ? Date.parse(endedAt) : now;
  if (Number.isNaN(end)) return null;
  return Math.max(0, end - start);
}

/**
 * Where a run stands among its steps: the position of the step in progress,
 * from 1, or the number of steps done when none is in progress.
 */
export function stepPosition(steps: WorkflowStep[]): { current: number; total: number } {
  const ordered = [...steps].sort((a, b) => a.rank - b.rank);
  const running = ordered.findIndex((step) => step.status === "running");
  const done = ordered.filter((step) => step.status === "completed").length;
  return { current: running >= 0 ? running + 1 : done, total: ordered.length };
}

/** The share of the steps of a run that are done, from 0 to 100. */
export function progressOf(steps: WorkflowStep[]): number {
  if (steps.length === 0) return 0;
  const done = steps.filter((step) => step.status === "completed").length;
  return Math.round((done / steps.length) * 100);
}

function inputText(value: unknown): string {
  if (typeof value === "string") return value;
  if (Array.isArray(value)) return value.map(inputText).join(", ");
  return JSON.stringify(value) ?? "";
}

/** The inputs of a run on one line, `name: value` joined by a middle dot. */
export function inputsSummary(inputs: Record<string, unknown> | null | undefined): string {
  if (!inputs) return "";
  return Object.entries(inputs)
    .map(([name, value]) => `${name}: ${inputText(value)}`)
    .join(" · ");
}

/** The package and the name of the workflow `<package>/<workflow>`. */
export function splitWorkflowId(workflowId: string): { package: string; name: string } {
  const cut = workflowId.indexOf("/");
  if (cut < 0) return { package: "", name: workflowId };
  return { package: workflowId.slice(0, cut), name: workflowId.slice(cut + 1) };
}

/** What a workflow may do, as the package that declares it says. */
export interface WorkflowAccess {
  /** `read`, `read_personal`, `write_or_delete`, `send_external` or `execute_code`. */
  family: string;
  /** Every family except `read` asks for the user's agreement. */
  sensitive: boolean;
}

const FAMILIES: readonly string[] = [
  "read",
  "read_personal",
  "write_or_delete",
  "send_external",
  "execute_code",
];

/**
 * The action family the workflow `name` declares in the description of its
 * package, `null` when the package does not describe it.
 */
export function workflowAccess(
  description: Record<string, unknown> | null | undefined,
  name: string,
): WorkflowAccess | null {
  const declared = description?.["workflows"];
  if (!Array.isArray(declared)) return null;
  for (const entry of declared) {
    if (typeof entry !== "object" || entry === null) continue;
    const workflow = entry as Record<string, unknown>;
    if (workflow["name"] !== name) continue;
    const family = workflow["action"];
    if (typeof family !== "string" || !FAMILIES.includes(family)) return null;
    return { family, sensitive: family !== "read" };
  }
  return null;
}

/** Translation key of an action family, shared with the sheet of an agent. */
export function familyLabelKey(family: string): string {
  return FAMILIES.includes(family) ? `agent_sheet.family_${family}` : "agent_sheet.family_unclassified";
}

/**
 * The workflows a filter keeps: those whose name, identifier or description
 * holds the query, whatever its case.
 */
export function filterWorkflows<T extends { workflowId: string; name: string; description: string }>(
  workflows: T[],
  query: string,
): T[] {
  const q = query.trim().toLowerCase();
  if (q === "") return workflows;
  return workflows.filter(
    (workflow) =>
      workflow.name.toLowerCase().includes(q) ||
      workflow.workflowId.toLowerCase().includes(q) ||
      workflow.description.toLowerCase().includes(q),
  );
}
