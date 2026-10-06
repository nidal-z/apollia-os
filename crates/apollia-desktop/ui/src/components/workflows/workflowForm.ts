/**
 * The launch form of a workflow, as decisions: which control an input takes,
 * the value a control starts with, which values are refused before the
 * launch, the inputs object a launch sends, and the reading of a refusal the
 * workflow engine answers. No i18n, no IPC.
 */
import type { WorkflowField, WorkflowInputKind } from "$lib/ipc/workflows";

/** The control a field is rendered with. */
export type FieldControl =
  | "text"
  | "textarea"
  | "file"
  | "date"
  | "number"
  | "toggle"
  | "select"
  | "checks"
  | "items"
  | "json";

/** What a control holds: a text, a switch, or the items of a list. */
export type FieldValue = string | boolean | string[];

/** The values of a form, by input name. */
export type FormValues = Record<string, FieldValue>;

/** Why a value is refused, as a key suffix of `workflows.error_*`. */
export type FieldErrorCode =
  | "required"
  | "not_an_integer"
  | "not_a_number"
  | "malformed_date"
  | "not_a_choice"
  | "not_json"
  | "wrong_type"
  | "missing_file"
  | "null"
  | "unknown";

/** Translation key of each reason a value is refused. */
export const FIELD_ERROR_LABEL: Record<FieldErrorCode, string> = {
  required: "workflows.error_required",
  not_an_integer: "workflows.error_not_an_integer",
  not_a_number: "workflows.error_not_a_number",
  malformed_date: "workflows.error_malformed_date",
  not_a_choice: "workflows.error_not_a_choice",
  not_json: "workflows.error_not_json",
  wrong_type: "workflows.error_wrong_type",
  missing_file: "workflows.error_missing_file",
  null: "workflows.error_null",
  unknown: "workflows.error_unknown",
};

/** A refusal of the workflow engine, read from its message. */
export interface LaunchRefusal {
  /** The input at fault. */
  field: string;
  code: FieldErrorCode;
  /** The engine's own sentence. */
  message: string;
}

const DAY = /^\d{4}-\d{2}-\d{2}$/;
const INTEGER = /^[+-]?\d+$/;
const REFUSAL = /input '([^']+)': (.*) \[([a-z_]+)\]/s;
const ENGINE_CODES: readonly FieldErrorCode[] = [
  "required",
  "unknown",
  "null",
  "wrong_type",
  "not_a_choice",
  "malformed_date",
  "missing_file",
];

/** The control `field` is rendered with. */
export function controlOf(field: WorkflowField): FieldControl {
  switch (field.kind) {
    case "long_text":
      return "textarea";
    case "file":
      return "file";
    case "date":
      return "date";
    case "integer":
    case "number":
      return "number";
    case "boolean":
      return "toggle";
    case "choice":
      return "select";
    case "list":
      return (field.choices?.length ?? 0) > 0 ? "checks" : "items";
    case "other":
      return "json";
    default:
      return "text";
  }
}

/** The value the control of `field` holds before anything is typed. */
export function emptyValue(field: WorkflowField): FieldValue {
  const control = controlOf(field);
  if (control === "toggle") return false;
  if (control === "checks" || control === "items") return [];
  return "";
}

function asText(value: unknown): string {
  if (typeof value === "string") return value;
  return JSON.stringify(value) ?? "";
}

/** The value of the control of `field` that shows `input`, an input of a past run. */
function valueFrom(field: WorkflowField, input: unknown): FieldValue {
  if (input === undefined || input === null) return emptyValue(field);
  const control = controlOf(field);
  if (control === "toggle") return input === true || input === "true";
  if (control === "checks" || control === "items") {
    return Array.isArray(input) ? input.map(asText) : [asText(input)];
  }
  return asText(input);
}

/**
 * The values a form opens with: empty, or the inputs of a past run when the
 * form is opened to launch it again.
 */
export function initialValues(
  fields: WorkflowField[],
  previous?: Record<string, unknown>,
): FormValues {
  const values: FormValues = {};
  for (const field of fields) {
    values[field.name] = previous ? valueFrom(field, previous[field.name]) : emptyValue(field);
  }
  return values;
}

function isEmpty(value: FieldValue): boolean {
  if (typeof value === "boolean") return false;
  if (Array.isArray(value)) return value.length === 0;
  return value.trim() === "";
}

function itemError(kind: WorkflowInputKind | undefined, item: string): FieldErrorCode | null {
  if (kind === "integer") return INTEGER.test(item.trim()) ? null : "not_an_integer";
  if (kind === "number") return Number.isFinite(Number(item.trim())) ? null : "not_a_number";
  if (kind === "date") return DAY.test(item.trim()) ? null : "malformed_date";
  return null;
}

/** Why the value of `field` is refused, `null` when it is not. */
export function fieldError(field: WorkflowField, value: FieldValue | undefined): FieldErrorCode | null {
  if (value === undefined || isEmpty(value)) return field.required ? "required" : null;
  if (typeof value === "boolean") return null;
  if (Array.isArray(value)) {
    const choices = field.choices ?? [];
    for (const item of value) {
      if (choices.length > 0 && !choices.includes(item)) return "not_a_choice";
      const refused = itemError(field.itemKind, item);
      if (refused) return refused;
    }
    return null;
  }
  const text = value.trim();
  switch (field.kind) {
    case "integer":
      return INTEGER.test(text) ? null : "not_an_integer";
    case "number":
      return Number.isFinite(Number(text)) ? null : "not_a_number";
    case "date":
      return DAY.test(text) ? null : "malformed_date";
    case "choice":
      return (field.choices ?? []).includes(text) ? null : "not_a_choice";
    case "other":
      try {
        JSON.parse(text);
        return null;
      } catch {
        return "not_json";
      }
    default:
      return null;
  }
}

/** The fields whose value is refused, each with why, by input name. */
export function formErrors(
  fields: WorkflowField[],
  values: FormValues,
): Record<string, FieldErrorCode> {
  const errors: Record<string, FieldErrorCode> = {};
  for (const field of fields) {
    const refused = fieldError(field, values[field.name]);
    if (refused) errors[field.name] = refused;
  }
  return errors;
}

function typedItem(kind: WorkflowInputKind | undefined, item: string): unknown {
  if (kind === "integer" || kind === "number") return Number(item.trim());
  if (kind === "boolean") return item === "true";
  return item;
}

/**
 * The inputs object a launch sends: each value in the type its input
 * declares, an optional input left empty left out. A switch is always sent,
 * since it always holds an answer.
 */
export function toInputs(fields: WorkflowField[], values: FormValues): Record<string, unknown> {
  const inputs: Record<string, unknown> = {};
  for (const field of fields) {
    const value = values[field.name];
    if (value === undefined || isEmpty(value)) continue;
    if (typeof value === "boolean") {
      inputs[field.name] = value;
    } else if (Array.isArray(value)) {
      inputs[field.name] = value.map((item) => typedItem(field.itemKind, item));
    } else if (field.kind === "integer" || field.kind === "number") {
      inputs[field.name] = Number(value.trim());
    } else if (field.kind === "other") {
      inputs[field.name] = JSON.parse(value);
    } else if (field.kind === "long_text") {
      inputs[field.name] = value;
    } else {
      inputs[field.name] = value.trim();
    }
  }
  return inputs;
}

/**
 * The refusal carried by a launch error, `null` when the error is not about
 * one input: the engine writes `input '<name>': <why> [<code>]`.
 */
export function parseRefusal(message: string): LaunchRefusal | null {
  const found = REFUSAL.exec(message);
  if (!found) return null;
  const code = ENGINE_CODES.find((known) => known === found[3]) ?? "wrong_type";
  return { field: found[1], code, message: found[2] };
}

/** The items of a list with `item` added, trimmed, once. */
export function addItem(items: string[], item: string): string[] {
  const text = item.trim();
  if (text === "" || items.includes(text)) return items;
  return [...items, text];
}

/** The items of a list with `item` checked or not. */
export function toggleItem(items: string[], item: string, on: boolean): string[] {
  if (on) return items.includes(item) ? items : [...items, item];
  return items.filter((kept) => kept !== item);
}
