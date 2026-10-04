/**
 * Typed Tauri command wrappers behind the workflows of the Agents screen:
 * the list, one workflow with the fields of its inputs, its runs, one run
 * with its steps, a launch and a stop.
 *
 * Thin wrappers over `crates/apollia-desktop/src/commands/workflows.rs`.
 */
import { invoke } from "@tauri-apps/api/core";

/** One installed workflow, as the list shows it. */
export interface WorkflowListItem {
  /** `<package>/<workflow>`. */
  workflowId: string;
  name: string;
  description: string;
}

/** The field a launch form shows for an input. */
export type WorkflowInputKind =
  | "text"
  | "long_text"
  | "file"
  | "date"
  | "integer"
  | "number"
  | "boolean"
  | "choice"
  | "list"
  | "other";

/** One input of a workflow, as its launch form shows it. */
export interface WorkflowField {
  /** The key of the inputs object. */
  name: string;
  kind: WorkflowInputKind;
  required: boolean;
  nullable: boolean;
  description?: string;
  /** The values a choice, or each item of a list of choices, may take. */
  choices?: string[];
  /** The kind of each item of a list. */
  itemKind?: WorkflowInputKind;
}

/** A workflow as its package declares it. */
export interface WorkflowView {
  /** `<package>/<workflow>`. */
  workflowId: string;
  /** The version of the package that declares it. */
  version: string;
  description: string;
  /** The declared steps, in order. */
  steps: string[];
  /** One field per input, in the declared order. */
  inputs: WorkflowField[];
}

/** How a run stands. */
export type WorkflowRunStatus = "running" | "completed" | "failed" | "canceled";

/** One run of a workflow. */
export interface WorkflowRun {
  runId: string;
  workflowId: string;
  /** The run whose step started this one, when one did. */
  parentRunId: string | null;
  depth: number;
  inputs: Record<string, unknown>;
  status: WorkflowRunStatus | (string & {});
  output: unknown;
  /** Why it failed or stopped. */
  error: string | null;
  startedAt: string;
  endedAt: string | null;
}

/** How a step of a run stands. */
export type WorkflowStepStatus =
  | "pending"
  | "running"
  | "completed"
  | "failed"
  | "canceled"
  | "not_started";

/** One declared step of a run. */
export interface WorkflowStep {
  /** Its position among the declared steps, from 0. */
  rank: number;
  name: string;
  status: WorkflowStepStatus | (string & {});
  startedAt: string | null;
  endedAt: string | null;
  output: string | null;
  error: string | null;
}

/** A run with its steps. */
export interface WorkflowRunDetail {
  run: WorkflowRun;
  steps: WorkflowStep[];
}

/** Every installed workflow. */
export function listWorkflows(): Promise<WorkflowListItem[]> {
  return invoke<WorkflowListItem[]>("list_workflows");
}

/** The workflow `workflowId`: what it does, its steps, the fields of its inputs. */
export function getWorkflow(workflowId: string): Promise<WorkflowView> {
  return invoke<WorkflowView>("get_workflow", { workflowId });
}

/** The runs of the workflow `workflowId`, newest first. */
export function listWorkflowRuns(workflowId: string, limit?: number): Promise<WorkflowRun[]> {
  return invoke<WorkflowRun[]>("list_workflow_runs", { workflowId, limit });
}

/**
 * Launches the workflow `workflowId` on `inputs`. An input the workflow
 * refuses rejects with a message that names it and says why.
 */
export function launchWorkflow(
  workflowId: string,
  inputs: Record<string, unknown>,
): Promise<WorkflowRun> {
  return invoke<WorkflowRun>("launch_workflow", { workflowId, inputs });
}

/** The run `runId` with its steps. */
export function getWorkflowRun(runId: string): Promise<WorkflowRunDetail> {
  return invoke<WorkflowRunDetail>("get_workflow_run", { runId });
}

/** Stops the run `runId`; resolves once it has ended. */
export function cancelWorkflowRun(runId: string): Promise<WorkflowRun> {
  return invoke<WorkflowRun>("cancel_workflow_run", { runId });
}
