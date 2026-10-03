/**
 * The target of an automation: an agent, which receives a message, or a
 * workflow, which receives its inputs.
 *
 * Pure decisions shared by the creation dialog, the edit form, the list and
 * the journal: how a target becomes the request fields the trigger routes
 * read, which required inputs are still empty, and the identifier proposed
 * for a new automation. No i18n, no IPC.
 */
import type { CreateTriggerRequest, TriggerTargetKind } from "$lib/types";

/** One input a workflow declares. */
export interface WorkflowInputField {
  name: string;
  required: boolean;
  description?: string;
}

/** What an automation starts, as the dialog that creates it knows it. */
export type AutomationTarget =
  | { kind: "agent"; name: string }
  | { kind: "workflow"; name: string; fields: WorkflowInputField[] };

/** The request fields that carry the target. */
export type TargetRequestFields = Pick<
  CreateTriggerRequest,
  "agent" | "workflow" | "workflow_inputs" | "input_template"
>;

/** Translation key of the word for a kind of target. */
export function targetKindLabelKey(kind: TriggerTargetKind): string {
  return kind === "workflow" ? "triggers.workflow_label" : "triggers.agent_label";
}

/**
 * The target fields of a create or update request: the agent and its
 * message, or the workflow and the inputs that hold a value.
 */
export function targetRequestFields(
  target: AutomationTarget,
  message: string,
  inputs: Record<string, string>,
): TargetRequestFields {
  if (target.kind === "agent") {
    const fields: TargetRequestFields = { agent: target.name };
    const text = message.trim();
    if (text) fields.input_template = text;
    return fields;
  }
  const filled: Record<string, string> = {};
  for (const [name, value] of Object.entries(inputs)) {
    const text = value.trim();
    if (text) filled[name] = text;
  }
  return { workflow: target.name, workflow_inputs: filled };
}

/** The required inputs of a workflow target that are still empty. */
export function missingInputs(
  target: AutomationTarget,
  inputs: Record<string, string>,
): string[] {
  if (target.kind !== "workflow") return [];
  return target.fields
    .filter((field) => field.required && !(inputs[field.name] ?? "").trim())
    .map((field) => field.name);
}

/**
 * The identifier proposed for an automation of `name` on `sourceType`:
 * lowercase, hyphenated, the grammar the trigger store accepts.
 */
export function suggestedTriggerId(name: string, sourceType: string): string {
  const slug = name
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
  const source = sourceType.replace(/_/g, "-");
  return slug ? `${slug}-${source}` : source;
}
