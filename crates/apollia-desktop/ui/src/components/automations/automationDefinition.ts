/**
 * Form model shared by the automation definition surfaces (create and edit).
 *
 * The backend entity is a "trigger" with a `source_type` plus an untyped
 * `source_config` JSON blob. This module is the single place that translates
 * between that stored shape and the flat field set a form can bind to, so the
 * edit path cannot drift from the create path.
 *
 * Everything here is pure: no i18n, no IPC. Validation returns translation
 * keys and the caller resolves them.
 */
import type { TriggerSourceInput, UpdateTriggerRequest } from "$lib/types";
import type { RedactedTriggerDefinition } from "$lib/ipc/triggers";

/** Source kinds the definition form can build. */
export type AutomationSourceType =
  | "cron"
  | "interval"
  | "oneshot"
  | "file_watch"
  | "webhook";

/** Flat, bindable projection of a trigger definition. */
export interface AutomationFormModel {
  /** Trigger identifier. Immutable once the trigger exists. */
  id: string;
  /** Target agent name. */
  agent: string;
  sourceType: AutomationSourceType;
  enabled: boolean;
  onBusy: "queue" | "drop";
  inputTemplate: string;
  cronSchedule: string;
  intervalEvery: string;
  /** `YYYY-MM-DD` half of a one-shot fire date. */
  oneshotDate: string;
  /** `HH:MM` half of a one-shot fire date. */
  oneshotTime: string;
  filePath: string;
  fileCreate: boolean;
  fileModify: boolean;
  fileDelete: boolean;
  fileRecursive: boolean;
  /**
   * Secret typed or generated during this session.
   *
   * Empty means "keep whatever is already stored", filled means "replace it".
   * The stored value never reaches this model: the host strips it from the
   * definition and completes the update payload itself.
   */
  webhookSecret: string;
  /** True when the trigger already holds a webhook secret host-side. */
  hasStoredWebhookSecret: boolean;
  /** Source type the definition was loaded with, `null` for a new trigger. */
  originalSourceType: AutomationSourceType | null;
  /** Raw stored `source_config`, so unknown keys survive a round-trip. */
  originalSourceConfig: Record<string, unknown>;
}

/** Identifier grammar accepted by the trigger repository. */
export const TRIGGER_ID_PATTERN = /^[a-z0-9][a-z0-9-]*[a-z0-9]$|^[a-z0-9]$/;

/** Blank model used by the create path. */
export function emptyAutomationModel(): AutomationFormModel {
  return {
    id: "",
    agent: "",
    sourceType: "cron",
    enabled: true,
    onBusy: "queue",
    inputTemplate: "",
    cronSchedule: "",
    intervalEvery: "",
    oneshotDate: "",
    oneshotTime: "",
    filePath: "",
    fileCreate: true,
    fileModify: true,
    fileDelete: false,
    fileRecursive: false,
    webhookSecret: "",
    hasStoredWebhookSecret: false,
    originalSourceType: null,
    originalSourceConfig: {},
  };
}

function readString(config: Record<string, unknown>, key: string): string {
  const value = config[key];
  return typeof value === "string" ? value : "";
}

function readBoolean(config: Record<string, unknown>, key: string): boolean {
  return config[key] === true;
}

function readStringArray(config: Record<string, unknown>, key: string): string[] {
  const value = config[key];
  if (!Array.isArray(value)) return [];
  return value.filter((item): item is string => typeof item === "string");
}

function isKnownSourceType(value: string): value is AutomationSourceType {
  return (
    value === "cron" ||
    value === "interval" ||
    value === "oneshot" ||
    value === "file_watch" ||
    value === "webhook"
  );
}

/**
 * Splits a stored `fire_at` into the date and time halves the pickers bind to.
 *
 * The create path writes a naive local `YYYY-MM-DDTHH:MM`; a definition posted
 * by an API client can carry a zoned RFC3339 stamp instead. Zoned values are
 * converted to local wall-clock so the operator reads the time they will get.
 */
export function splitFireAt(value: string): { date: string; time: string } {
  if (!value) return { date: "", time: "" };
  if (/(?:Z|[+-]\d{2}:?\d{2})$/.test(value)) {
    const parsed = new Date(value);
    if (!Number.isNaN(parsed.getTime())) {
      const pad = (n: number) => String(n).padStart(2, "0");
      return {
        date: `${parsed.getFullYear()}-${pad(parsed.getMonth() + 1)}-${pad(parsed.getDate())}`,
        time: `${pad(parsed.getHours())}:${pad(parsed.getMinutes())}`,
      };
    }
  }
  return { date: value.slice(0, 10), time: value.slice(11, 16) };
}

/**
 * Joins the picker halves into the timestamp the API validates.
 *
 * The repository parses `fire_at` as a zoned RFC3339 stamp and rejects a naive
 * one, so the local wall-clock the operator picked is converted to UTC here.
 */
export function toFireAtIso(date: string, time: string): string {
  if (!date || !time) return "";
  const local = new Date(`${date}T${time}`);
  if (Number.isNaN(local.getTime())) return "";
  return local.toISOString();
}

/** Projects a stored definition onto the form model. */
export function modelFromDefinition(
  definition: RedactedTriggerDefinition,
): AutomationFormModel {
  const model = emptyAutomationModel();
  const config = definition.source_config ?? {};

  model.id = definition.id;
  model.agent = definition.agent ?? "";
  model.enabled = definition.enabled;
  model.onBusy = definition.on_busy === "drop" ? "drop" : "queue";
  model.inputTemplate = definition.input_template ?? "";
  model.originalSourceConfig = config;

  if (isKnownSourceType(definition.source_type)) {
    model.sourceType = definition.source_type;
    model.originalSourceType = definition.source_type;
  }

  switch (model.sourceType) {
    case "cron":
      model.cronSchedule = readString(config, "schedule");
      break;
    case "interval":
      model.intervalEvery = readString(config, "every");
      break;
    case "oneshot": {
      const parts = splitFireAt(readString(config, "fire_at"));
      model.oneshotDate = parts.date;
      model.oneshotTime = parts.time;
      break;
    }
    case "file_watch": {
      model.filePath = readString(config, "path");
      const events = readStringArray(config, "events");
      model.fileCreate = events.includes("create") || events.includes("any");
      model.fileModify = events.includes("modify") || events.includes("any");
      model.fileDelete = events.includes("delete") || events.includes("any");
      model.fileRecursive = readBoolean(config, "recursive");
      break;
    }
    case "webhook":
      // The value itself never arrives, only the marker the host substitutes.
      model.hasStoredWebhookSecret = definition.has_secret === true;
      break;
  }

  return model;
}

/**
 * Secret to put in the payload: the rotated one, or an empty string.
 *
 * Empty is the wire convention for "keep the stored secret". The host resolves
 * it against the definition store before the update reaches the API.
 */
export function effectiveWebhookSecret(model: AutomationFormModel): string {
  return model.webhookSecret.trim();
}

function buildBaseSource(model: AutomationFormModel): TriggerSourceInput | null {
  switch (model.sourceType) {
    case "cron":
      return { type: "cron", schedule: model.cronSchedule.trim() };
    case "interval":
      return { type: "interval", every: model.intervalEvery.trim() };
    case "oneshot":
      return {
        type: "oneshot",
        fire_at: toFireAtIso(model.oneshotDate, model.oneshotTime),
      };
    case "file_watch": {
      const events: string[] = [];
      if (model.fileCreate) events.push("create");
      if (model.fileModify) events.push("modify");
      if (model.fileDelete) events.push("delete");
      return {
        type: "file_watch",
        path: model.filePath.trim(),
        events,
        recursive: model.fileRecursive,
      };
    }
    case "webhook":
      return { type: "webhook", secret: effectiveWebhookSecret(model) };
  }
}

/**
 * Builds the source payload sent to the API.
 *
 * When the source type is unchanged, keys the form does not manage (for
 * instance `exclude_patterns` on a file watch) are carried over so an edit
 * never silently drops configuration the form cannot show.
 */
export function buildSource(model: AutomationFormModel): TriggerSourceInput | null {
  const base = buildBaseSource(model);
  if (!base) return null;
  if (model.originalSourceType !== model.sourceType) return base;
  const merged: Record<string, unknown> = { ...model.originalSourceConfig, ...base };
  // The merge only adds keys the typed union does not describe; the required
  // ones all come from `base`, hence the widening round-trip.
  return merged as unknown as TriggerSourceInput;
}

/** Translation keys of the current validation failures, `null` when valid. */
export interface AutomationFormErrors {
  id: string | null;
  target: string | null;
  source: string | null;
}

/** Validates the model. `mode` relaxes the rules the edit path cannot enforce. */
export function validateModel(
  model: AutomationFormModel,
  mode: "create" | "edit",
): AutomationFormErrors {
  const errors: AutomationFormErrors = { id: null, target: null, source: null };

  if (!model.id.trim()) {
    errors.id = "triggers.field_id_required";
  } else if (!TRIGGER_ID_PATTERN.test(model.id)) {
    errors.id = "triggers.field_id_invalid";
  }

  if (!model.agent) {
    errors.target = "triggers.field_target_required";
  }

  switch (model.sourceType) {
    case "cron":
      if (!model.cronSchedule.trim()) errors.source = "triggers.field_schedule_required";
      break;
    case "interval":
      if (!model.intervalEvery.trim()) errors.source = "triggers.field_every_required";
      break;
    case "oneshot":
      if (!model.oneshotDate || !model.oneshotTime) {
        errors.source = "triggers.field_fire_at_required";
      }
      break;
    case "file_watch":
      if (!model.filePath.trim()) {
        errors.source = "triggers.field_path_required";
      } else if (!model.fileCreate && !model.fileModify && !model.fileDelete) {
        errors.source = "triggers.field_events_required";
      }
      break;
    case "webhook":
      // An empty field on the edit path means "keep the stored secret", which
      // only holds when one is actually stored.
      if (
        !effectiveWebhookSecret(model) &&
        !(mode === "edit" && model.hasStoredWebhookSecret)
      ) {
        errors.source = "triggers.field_secret_required";
      }
      break;
  }

  // An edit keeps the identifier the trigger was created with, so a stored id
  // that predates the current grammar must not block a schedule change.
  if (mode === "edit" && model.id.trim()) {
    errors.id = null;
  }

  return errors;
}

/** True when no field error blocks a submit. */
export function isModelValid(errors: AutomationFormErrors): boolean {
  return !errors.id && !errors.target && !errors.source;
}

/** Builds the `PUT /triggers/:id` payload, `null` when the source is unbuildable. */
export function toUpdateRequest(model: AutomationFormModel): UpdateTriggerRequest | null {
  const source = buildSource(model);
  if (!source) return null;
  const request: UpdateTriggerRequest = {
    agent: model.agent,
    enabled: model.enabled,
    on_busy: model.onBusy,
    source,
  };
  const template = model.inputTemplate.trim();
  if (template) {
    request.input_template = template;
  }
  return request;
}

/** Hex secret with the same shape the create path generates. */
export function generateWebhookSecret(): string {
  const bytes = new Uint8Array(16);
  crypto.getRandomValues(bytes);
  return Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
}
