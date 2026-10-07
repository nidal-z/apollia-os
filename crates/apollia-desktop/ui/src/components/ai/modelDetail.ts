/**
 * The decisions of the panel of a model, kept out of its components so they
 * are tested in `node`: what the three tabs show, how a setting left alone
 * reads, what a preset is written with, when a loaded model is said to be
 * relaunched, and which sections exist only in developer mode.
 *
 * Pure and i18n-agnostic: labels are catalogue keys the caller resolves.
 */
import type { InstalledModel, ModelCategory } from "$lib/ipc/modelHub";
import type {
  AuthorField,
  ModelDetail,
  PresetDetail,
  PresetRequestSettings,
  PresetWrite,
} from "$lib/ipc/modelDetail";
import type { ModelTarget } from "$lib/chat/chatTargets";
import type { AiModelSelection } from "$lib/stores/aiModelPanel";
import { categoryLabelKey } from "./aiModels";

/** The tabs of the panel, in order. */
export type PanelTab = "info" | "load" | "inference";

export const PANEL_TABS: readonly PanelTab[] = ["info", "load", "inference"];

const TAB_LABEL_KEYS: Record<PanelTab, string> = {
  info: "ai.panel.tab.info",
  load: "ai.panel.tab.load",
  inference: "ai.panel.tab.inference",
};

/** The catalogue key of the name of a tab. */
export function tabLabelKey(tab: PanelTab): string {
  return TAB_LABEL_KEYS[tab];
}

/**
 * The tabs the panel offers: Infos for everyone, Loading and Inference with
 * the developer mode on. Everything they hold is automatic by default, so
 * nobody has to reach them for a model to work.
 */
export function panelTabs(developerMode: boolean): PanelTab[] {
  return developerMode ? [...PANEL_TABS] : ["info"];
}

/** The tab on screen: the one asked for when the mode offers it, Infos otherwise. */
export function shownPanelTab(tab: PanelTab, developerMode: boolean): PanelTab {
  return panelTabs(developerMode).includes(tab) ? tab : "info";
}

/**
 * What the panel shows: the model named `openId` as the list has it now, and
 * whether it is in memory. `null` when no panel is open or the model left
 * the list.
 */
export function panelSelection(
  installed: readonly InstalledModel[],
  loadedPaths: ReadonlySet<string>,
  openId: string | null,
): AiModelSelection | null {
  if (openId === null) return null;
  const model = installed.find((m) => m.id === openId);
  return model ? { model, loaded: loadedPaths.has(model.path) } : null;
}

// ── Infos ────────────────────────────────────────────────────────────────

/** The lines of the Infos tab, in the order the screen shows them. */
export type InfoRowId =
  | "model"
  | "file"
  | "format"
  | "quantization"
  | "architecture"
  | "capabilities"
  | "domain"
  | "size";

export const INFO_ROWS: readonly InfoRowId[] = [
  "model",
  "file",
  "format",
  "quantization",
  "architecture",
  "capabilities",
  "domain",
  "size",
];

/** One line of the Infos tab. */
export interface InfoRow {
  id: InfoRowId;
  labelKey: string;
  /** The text shown as written; `null` for a line drawn another way. */
  value: string | null;
  /** The catalogue key of the text, when the value is a word to translate. */
  valueKey?: string;
}

/**
 * The lines of the Infos tab. What the weights do not say reads "unknown";
 * the capabilities and the size are drawn by the component (icons, a number
 * in the reader's locale).
 */
export function infoRows(model: InstalledModel, detail: ModelDetail | null): InfoRow[] {
  const unknown = "ai.panel.info.unknown";
  const text = (id: InfoRowId, value: string | null | undefined): InfoRow =>
    value
      ? { id, labelKey: `ai.panel.info.${id}`, value }
      : { id, labelKey: `ai.panel.info.${id}`, value: null, valueKey: unknown };
  return [
    text("model", model.id),
    text("file", detail?.file ?? model.name),
    text("format", model.format.toUpperCase()),
    text("quantization", detail?.quantization),
    text("architecture", detail?.architecture),
    { id: "capabilities", labelKey: "ai.panel.info.capabilities", value: null },
    {
      id: "domain",
      labelKey: "ai.panel.info.domain",
      value: null,
      valueKey: categoryLabelKey(model.category),
    },
    { id: "size", labelKey: "ai.panel.info.size", value: null },
  ];
}

// ── The head of the panel ────────────────────────────────────────────────

/** One button of the head of the panel. */
export interface HeadAction {
  enabled: boolean;
  /** Why the button cannot be used, shown on hover; absent when it can. */
  reasonKey?: string;
}

/** The two buttons of the head. */
export interface HeadActions {
  chat: HeadAction;
  /** "Load", or "Unload" once the model is in memory. */
  load: HeadAction & { kind: "load" | "unload" };
}

function converses(category: ModelCategory): boolean {
  return category === "language" || category === "draft";
}

/**
 * What the head offers: a conversation opens on a model that converses, and
 * the loading window opens on a model of that kind that is not in memory.
 */
export function headActions(model: InstalledModel, loaded: boolean): HeadActions {
  const reasonKey =
    model.category === "embeddings"
      ? "ai.panel.head.reason_embeddings"
      : "ai.panel.head.reason_dictation";
  if (!converses(model.category)) {
    return {
      chat: { enabled: false, reasonKey },
      load: { kind: "load", enabled: false, reasonKey },
    };
  }
  return {
    chat: { enabled: true },
    load: { kind: loaded ? "unload" : "load", enabled: true },
  };
}

/**
 * The target "Use in a new conversation" starts a conversation on: this
 * model, named as the selector of a conversation names it.
 */
export function chatTargetOf(model: InstalledModel): ModelTarget {
  return {
    kind: "model",
    route: model.id,
    label: model.id,
    source: "local",
    format: model.format,
    server: null,
    parameters: model.parameters,
    path: model.path,
  };
}

// ── Settings ─────────────────────────────────────────────────────────────

/** How a setting is typed in. */
export type FieldKind =
  | "integer"
  | "number"
  | "boolean"
  | "choice"
  | "text"
  | "multiline"
  | "lines"
  | "draft";

/** One setting of a preset. */
export interface SettingField {
  /** The key the preset carries. */
  key: string;
  kind: FieldKind;
  section: string;
  labelKey: string;
  /** What the setting changes, shown on hover. */
  helpKey: string;
  min?: number;
  max?: number;
  step?: number;
  /** The values of a `choice`. */
  choices?: readonly string[];
}

function field(
  scope: "load" | "inference",
  section: string,
  key: string,
  kind: FieldKind,
  extra: Partial<SettingField> = {},
): SettingField {
  return {
    key,
    kind,
    section,
    labelKey: `ai.panel.${scope}.field.${key}.label`,
    helpKey: `ai.panel.${scope}.field.${key}.help`,
    ...extra,
  };
}

/** The sections of the Loading tab of a GGUF model, in order. */
export const LOAD_SECTIONS = ["context", "speculative", "advanced"] as const;
export type LoadSection = (typeof LOAD_SECTIONS)[number];

const CACHE_TYPES = ["f32", "f16", "bf16", "q8_0", "q4_0", "q4_1", "iq4_nl", "q5_0", "q5_1"];

/** The load key that names the draft model, by its identifier in the model index. */
export const DRAFT_MODEL_KEY = "draft_model";

/**
 * The load settings of a GGUF model, by the keys the local engine reads from
 * a preset. The draft model is chosen among the drafts the model can use (see
 * `draftOptions`); the speculative mode is not here, since the runtime fixes
 * it as soon as a draft model is named.
 */
export const LOAD_FIELDS: readonly SettingField[] = [
  field("load", "context", "n_ctx", "integer", { min: 1024, step: 1024 }),
  field("load", "context", "n_gpu_layers", "integer", { min: -1 }),
  field("load", "speculative", DRAFT_MODEL_KEY, "draft"),
  field("load", "advanced", "n_threads", "integer", { min: 1 }),
  field("load", "advanced", "n_batch", "integer", { min: 1 }),
  field("load", "advanced", "n_ubatch", "integer", { min: 1 }),
  field("load", "advanced", "n_parallel", "integer", { min: 1 }),
  field("load", "advanced", "kv_unified", "boolean"),
  field("load", "advanced", "ctx_checkpoints", "integer", { min: 0 }),
  field("load", "advanced", "reasoning_budget", "integer", { min: -1 }),
  field("load", "advanced", "reasoning_budget_message", "text"),
  field("load", "advanced", "rope_freq_base", "number", { min: 0 }),
  field("load", "advanced", "rope_freq_scale", "number", { min: 0 }),
  field("load", "advanced", "kv_offload", "boolean"),
  field("load", "advanced", "mlock", "boolean"),
  field("load", "advanced", "mmap", "boolean"),
  field("load", "advanced", "seed", "integer", { min: 0 }),
  field("load", "advanced", "chat_template", "multiline"),
  field("load", "advanced", "flash_attn", "choice", { choices: ["on", "off", "auto"] }),
  field("load", "advanced", "cache_type_k", "choice", { choices: CACHE_TYPES }),
  field("load", "advanced", "cache_type_v", "choice", { choices: CACHE_TYPES }),
];

/** The sections of the Inference tab, in order. */
export const INFERENCE_SECTIONS = ["settings", "sampling", "structured"] as const;
export type InferenceSection = (typeof INFERENCE_SECTIONS)[number];

/**
 * The request settings of a preset. No system prompt: a preset never carries
 * one, and the route refuses it.
 */
export const INFERENCE_FIELDS: readonly SettingField[] = [
  field("inference", "settings", "temperature", "number", { min: 0, max: 2, step: 0.1 }),
  field("inference", "settings", "max_tokens", "integer", { min: 1 }),
  field("inference", "settings", "stop", "lines"),
  field("inference", "sampling", "top_k", "integer", { min: 0 }),
  field("inference", "sampling", "repeat_penalty", "number", { min: 0, step: 0.05 }),
  field("inference", "sampling", "presence_penalty", "number", { min: -2, max: 2, step: 0.1 }),
  field("inference", "sampling", "top_p", "number", { min: 0, max: 1, step: 0.05 }),
  field("inference", "sampling", "min_p", "number", { min: 0, max: 1, step: 0.05 }),
];

/** The fields of one section, in order. */
export function fieldsOf(fields: readonly SettingField[], section: string): SettingField[] {
  return fields.filter((f) => f.section === section);
}

/**
 * The sections the Loading tab shows for a model. The speculative decoding
 * section is there only when the engine of the model takes a draft model,
 * which the detail says; until the detail is read it is left out.
 */
export function loadSections(detail: ModelDetail | null): LoadSection[] {
  return LOAD_SECTIONS.filter((s) => s !== "speculative" || detail?.draft_setting === true);
}

/** One entry of the list of draft models. */
export interface DraftOption {
  /** The identifier the load settings store. */
  id: string;
  /** Its parameter count, when its header says it. */
  parameters: string | null;
  /** Whether the draft can still be used with this model. */
  available: boolean;
}

/**
 * The draft models offered for a model: the compatible ones, in the order of
 * the index. The draft a preset already names stays in the list when it is no
 * longer compatible or installed, marked unavailable, so the list shows what
 * the preset holds instead of silently reading "None".
 */
export function draftOptions(detail: ModelDetail | null, held: string | undefined): DraftOption[] {
  const options: DraftOption[] = (detail?.compatible_drafts ?? []).map((d) => ({
    id: d.id,
    parameters: d.parameters,
    available: true,
  }));
  const chosen = (held ?? "").trim();
  if (chosen !== "" && !options.some((o) => o.id === chosen)) {
    options.push({ id: chosen, parameters: null, available: false });
  }
  return options;
}

/** Whether the draft model the form names can no longer be used with the model. */
export function draftUnavailable(detail: ModelDetail | null, held: string | undefined): boolean {
  return draftOptions(detail, held).some((o) => !o.available);
}

/**
 * What the Loading tab shows: the settings of a GGUF model, or the sentence
 * that says an MLX model takes none.
 */
export function loadTabView(format: "gguf" | "mlx"): "settings" | "none" {
  return format === "mlx" ? "none" : "settings";
}

/** What the panel holds of a preset: every setting as typed, `""` for automatic. */
export interface PanelForm {
  load: Record<string, string>;
  request: Record<string, string>;
  /** Whether the answer is held to a JSON Schema. */
  structured: boolean;
  /** The JSON Schema, as typed. */
  schema: string;
}

function draft(kind: FieldKind, value: unknown): string {
  if (value === undefined || value === null) return "";
  if (kind === "lines") return Array.isArray(value) ? value.map(String).join("\n") : "";
  if (kind === "boolean") return typeof value === "boolean" ? String(value) : "";
  return typeof value === "string" || typeof value === "number" ? String(value) : "";
}

/** The form of a preset; every setting automatic when there is none. */
export function formFromPreset(preset: PresetDetail | null): PanelForm {
  const load: Record<string, string> = {};
  for (const f of LOAD_FIELDS) load[f.key] = draft(f.kind, preset?.load[f.key]);
  const request: Record<string, string> = {};
  const held = (preset?.request ?? {}) as Record<string, unknown>;
  for (const f of INFERENCE_FIELDS) request[f.key] = draft(f.kind, held[f.key]);
  const schema = preset?.request.json_schema;
  return {
    load,
    request,
    structured: schema !== undefined && schema !== null,
    schema: schema === undefined || schema === null ? "" : JSON.stringify(schema, null, 2),
  };
}

/** Whether a setting is left to Apollia. */
export function isAuto(raw: string | undefined): boolean {
  return (raw ?? "").trim() === "";
}

/** What a typed value is worth: nothing (automatic), a value, or a refusal. */
export type Parsed =
  | { kind: "auto" }
  | { kind: "value"; value: unknown }
  | { kind: "invalid"; errorKey: string };

/** Reads what was typed for `f`. */
export function parseField(f: SettingField, raw: string | undefined): Parsed {
  const text = (raw ?? "").trim();
  if (text === "") return { kind: "auto" };
  switch (f.kind) {
    case "integer":
    case "number": {
      const n = Number(text.replace(",", "."));
      if (!Number.isFinite(n) || (f.kind === "integer" && !Number.isInteger(n))) {
        return {
          kind: "invalid",
          errorKey: f.kind === "integer" ? "ai.panel.error.integer" : "ai.panel.error.number",
        };
      }
      if ((f.min !== undefined && n < f.min) || (f.max !== undefined && n > f.max)) {
        return { kind: "invalid", errorKey: "ai.panel.error.range" };
      }
      return { kind: "value", value: n };
    }
    case "boolean":
      return { kind: "value", value: text === "true" };
    case "draft":
      return { kind: "value", value: text };
    case "choice":
      return f.choices?.includes(text)
        ? { kind: "value", value: text }
        : { kind: "invalid", errorKey: "ai.panel.error.choice" };
    case "lines": {
      const lines = (raw ?? "").split("\n").filter((l) => l.length > 0);
      return lines.length === 0 ? { kind: "auto" } : { kind: "value", value: lines };
    }
    default:
      return { kind: "value", value: raw ?? "" };
  }
}

/** The settings a form is worth, or the fields it cannot be read at. */
export type FormSettings =
  | { ok: true; load: Record<string, unknown>; request: PresetRequestSettings }
  | { ok: false; errors: Record<string, string> };

/** The key under which the error of the JSON Schema is reported. */
export const SCHEMA_ERROR_KEY = "json_schema";

/** Reads the whole form. A setting left automatic is not written at all. */
export function formSettings(form: PanelForm): FormSettings {
  const errors: Record<string, string> = {};
  const read = (fields: readonly SettingField[], drafts: Record<string, string>) => {
    const out: Record<string, unknown> = {};
    for (const f of fields) {
      const parsed = parseField(f, drafts[f.key]);
      if (parsed.kind === "invalid") errors[f.key] = parsed.errorKey;
      else if (parsed.kind === "value") out[f.key] = parsed.value;
    }
    return out;
  };
  const load = read(LOAD_FIELDS, form.load);
  const request = read(INFERENCE_FIELDS, form.request) as PresetRequestSettings;
  if (form.structured) {
    try {
      const schema: unknown = JSON.parse(form.schema);
      if (typeof schema !== "object" || schema === null || Array.isArray(schema)) {
        errors[SCHEMA_ERROR_KEY] = "ai.panel.error.schema_object";
      } else {
        request.json_schema = schema;
      }
    } catch {
      errors[SCHEMA_ERROR_KEY] = "ai.panel.error.schema_json";
    }
  }
  return Object.keys(errors).length > 0 ? { ok: false, errors } : { ok: true, load, request };
}

function sameDrafts(a: Record<string, string>, b: Record<string, string>): boolean {
  const keys = new Set([...Object.keys(a), ...Object.keys(b)]);
  for (const key of keys) if ((a[key] ?? "").trim() !== (b[key] ?? "").trim()) return false;
  return true;
}

/** Whether a load setting differs from the ones the panel opened with. */
export function loadChanged(form: PanelForm, baseline: PanelForm): boolean {
  return !sameDrafts(form.load, baseline.load);
}

/** Whether anything differs from what the panel opened with. */
export function formChanged(form: PanelForm, baseline: PanelForm): boolean {
  return (
    loadChanged(form, baseline) ||
    !sameDrafts(form.request, baseline.request) ||
    form.structured !== baseline.structured ||
    (form.structured && form.schema.trim() !== baseline.schema.trim())
  );
}

/**
 * Whether the panel warns that the model will be relaunched: a load setting
 * was changed while the model is in memory. A load setting only takes effect
 * when the engine starts the model again.
 */
export function relaunchWarning(loaded: boolean, form: PanelForm, baseline: PanelForm): boolean {
  return loaded && loadChanged(form, baseline);
}

// ── Presets ──────────────────────────────────────────────────────────────

/** The presets of one local model, by name. */
export function presetsOf(presets: readonly PresetDetail[], modelId: string): PresetDetail[] {
  return presets
    .filter((p) => p.model_source === "local" && p.model_id === modelId)
    .sort((a, b) => a.name.localeCompare(b.name));
}

/** The preset the panel opens on: the default of the model, when it has one. */
export function initialPreset(presets: readonly PresetDetail[]): PresetDetail | null {
  return presets.find((p) => p.is_model_default) ?? null;
}

/** Why a preset cannot be saved under `name`; `null` when it can. */
export function presetNameError(name: string, presets: readonly PresetDetail[]): string | null {
  const wanted = name.trim();
  if (wanted === "") return "ai.panel.preset.error_name_empty";
  if (presets.some((p) => p.name.trim().toLowerCase() === wanted.toLowerCase())) {
    return "ai.panel.preset.error_name_taken";
  }
  return null;
}

/**
 * The load settings a write carries. While presets do not keep load settings
 * (the setting of the developer mode is off), a new preset carries none and a
 * modified one keeps those it already holds, which is what the route accepts.
 */
export function writtenLoad(
  loadSettingsKept: boolean,
  typed: Record<string, unknown>,
  held: Record<string, unknown> | null,
): Record<string, unknown> {
  if (loadSettingsKept) return typed;
  return held ?? {};
}

/** What "Save as a preset" writes. The fields of the card are never part of it. */
export function newPresetWrite(
  name: string,
  settings: Extract<FormSettings, { ok: true }>,
  loadSettingsKept: boolean,
): PresetWrite {
  return {
    name: name.trim(),
    load: writtenLoad(loadSettingsKept, settings.load, null),
    request: settings.request,
    is_model_default: false,
  };
}

/** What saving the changes of `preset` writes. */
export function presetUpdateWrite(
  preset: PresetDetail,
  settings: Extract<FormSettings, { ok: true }>,
  loadSettingsKept: boolean,
): PresetWrite {
  return {
    name: preset.name,
    load: writtenLoad(loadSettingsKept, settings.load, preset.load),
    request: settings.request,
    is_model_default: preset.is_model_default,
  };
}

/** What "Set as default for this model" writes: the preset as held, made the default. */
export function defaultPresetWrite(preset: PresetDetail): PresetWrite {
  return { name: preset.name, load: preset.load, request: preset.request, is_model_default: true };
}

/** Whether "Set as default for this model" can be used. */
export function canSetDefault(preset: PresetDetail | null, changed: boolean): boolean {
  return preset !== null && !preset.is_model_default && !changed;
}

/** Whether the runtime refused a write because presets do not keep load settings. */
export function isLoadSettingsRefusal(message: string): boolean {
  return message.includes("models.preset_load_settings");
}

// ── Developer mode ───────────────────────────────────────────────────────

/** The sections shown under the tabs in developer mode only. */
export type DevSection = "type" | "source";

/**
 * The sections under the tabs: the type and the source file of the model with
 * the developer mode on, none with it off, so nothing of them is rendered.
 */
export function devSections(developerMode: boolean): DevSection[] {
  return developerMode ? ["type", "source"] : [];
}

/** One choice of "Model type": no override, or a category. */
export type TypeChoice = "none" | ModelCategory;

export const TYPE_CHOICES: readonly TypeChoice[] = [
  "none",
  "language",
  "embeddings",
  "dictation",
  "draft",
];

/** The choice in force. */
export function typeChoiceOf(detail: ModelDetail | null): TypeChoice {
  return detail?.category_override ?? "none";
}

/** The category a choice writes to the index: none removes the override. */
export function overrideOf(choice: TypeChoice): ModelCategory | null {
  return choice === "none" ? null : choice;
}

/** The catalogue key of the name of a type choice. */
export function typeLabelKey(choice: TypeChoice): string {
  return `dev.ai.panel.type.${choice}.label`;
}

/** The catalogue key of what a type choice means, shown on hover. */
export function typeExplanationKey(choice: TypeChoice): string {
  return `dev.ai.panel.type.${choice}.explanation`;
}

/** What the "Source file" section shows. */
export interface SourceView {
  /** Whether the override is on. */
  enabled: boolean;
  /** Whether the switch can be turned on: another weights file exists. */
  available: boolean;
  /** The path chosen, `null` while the override is off. */
  chosen: string | null;
}

/** The state of the source override of a model. */
export function sourceView(detail: ModelDetail | null): SourceView {
  const chosen = detail?.source_file_override ?? null;
  return {
    enabled: chosen !== null,
    available: (detail?.source_candidates.length ?? 0) > 0 || chosen !== null,
    chosen,
  };
}

/**
 * What turning the switch writes: on, the first other weights file; off, no
 * override. `undefined` when there is nothing to turn on.
 */
export function sourceToggle(detail: ModelDetail, on: boolean): string | null | undefined {
  if (!on) return null;
  return detail.source_candidates[0]?.path;
}

// ── Author fields ────────────────────────────────────────────────────────

/** One switch of the card, as the Inference tab reads it. */
export interface AuthorFieldRow {
  name: string;
  explanation: string;
  /** The default as written, for a value that is not a boolean. */
  defaultText: string | null;
  /** The catalogue key of the default of a boolean. */
  defaultKey: string | null;
  /** The values a list accepts, as written. */
  values: string[];
}

/** What a line reads of a switch of the card. */
export type AuthorFieldFacts = Pick<AuthorField, "name" | "default" | "explanation" | "values">;

function written(value: unknown): string {
  return typeof value === "string" ? value : JSON.stringify(value);
}

/**
 * The switches the card of the model declares, to read: their name, their
 * default and what they change. Nothing of them is sent to the engine.
 */
export function authorFieldRows(fields: readonly AuthorFieldFacts[]): AuthorFieldRow[] {
  return fields.map((f) => {
    const boolean = typeof f.default === "boolean";
    return {
      name: f.name,
      explanation: f.explanation,
      defaultText: boolean ? null : written(f.default),
      defaultKey: boolean
        ? f.default
          ? "ai.panel.inference.author.on"
          : "ai.panel.inference.author.off"
        : null,
      values: (f.values ?? []).map(written),
    };
  });
}
