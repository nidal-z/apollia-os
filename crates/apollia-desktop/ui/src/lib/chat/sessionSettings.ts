/**
 * The settings of one conversation, as the Settings tab of the chat reads
 * and writes them.
 *
 * A conversation is tuned without touching its agent: a field left out of
 * its settings is inherited. The preset chosen for the conversation carries
 * the temperature and the answer length; a value the conversation sets over
 * its preset is a modification, marked until it is saved as a preset or
 * discarded. The instructions never enter a preset.
 */

/** What a conversation does at the context limit of its model. */
export type ContextOverflow = "summarize_middle" | "sliding_window" | "stop_at_limit";

/** The three values, in the order the list shows them; the first is the default. */
export const CONTEXT_OVERFLOWS: readonly ContextOverflow[] = [
  "summarize_middle",
  "sliding_window",
  "stop_at_limit",
];

/** Level of reasoning of a model that reads one. */
export type ReasoningEffort = "low" | "medium" | "high";

export const REASONING_EFFORTS: readonly ReasoningEffort[] = ["low", "medium", "high"];

/** Longest instructions a conversation adds to the mission of its agent. */
export const INSTRUCTIONS_MAX_CHARS = 4000;

/** Longest notes a conversation keeps. */
export const NOTES_MAX_CHARS = 20000;

/** The answer length a conversation starts from when the limit is switched on. */
export const DEFAULT_MAX_TOKENS = 1024;

/** The settings of one conversation, as the runtime stores them. */
export interface SessionSettings {
  preset?: number;
  thinking?: boolean;
  reasoning_effort?: ReasoningEffort;
  /** The reasoning of the earlier answers goes back to the model; left out, it does not. */
  preserve_thinking?: boolean;
  temperature?: number;
  max_tokens?: number;
  context_overflow?: ContextOverflow;
  instructions?: string;
  disabled_accesses?: string[];
  proposed_accesses?: string[];
  profile_access?: boolean;
}

/** The request settings a preset carries; the panel reads two of them. */
export interface PresetRequest {
  temperature?: number;
  max_tokens?: number;
  [setting: string]: unknown;
}

/** A preset of a model, as the runtime lists it. */
export interface SessionPreset {
  id: number;
  name: string;
  model_source: "local" | "server";
  model_id: string;
  load?: Record<string, unknown>;
  request?: PresetRequest;
  is_model_default: boolean;
}

/** The model a conversation runs on, in the terms a preset is attached by. */
export interface SessionModel {
  model_source: "local" | "server";
  model_id: string;
}

/** What the runtime takes to create a preset. */
export interface PresetDraft {
  name: string;
  model_source: "local" | "server";
  model_id: string;
  load: Record<string, unknown>;
  request: PresetRequest;
}

/** A setting a preset carries, which a conversation can set over it. */
export type PresetField = "temperature" | "max_tokens";

export const PRESET_FIELDS: readonly PresetField[] = ["temperature", "max_tokens"];

/** The model of a conversation from what its capabilities answer. */
export function sessionModelOf(
  capabilities: { model: string | null; local: boolean } | null,
): SessionModel | null {
  if (!capabilities?.model) return null;
  return { model_source: capabilities.local ? "local" : "server", model_id: capabilities.model };
}

/** The presets of `model`, by name: the ones a conversation on it can choose. */
export function presetsOfModel(
  presets: readonly SessionPreset[],
  model: SessionModel | null,
): SessionPreset[] {
  if (!model) return [];
  return presets
    .filter((p) => p.model_source === model.model_source && p.model_id === model.model_id)
    .sort((a, b) => a.name.localeCompare(b.name));
}

/** The preset the conversation chose, `null` when it chose none or it is gone. */
export function chosenPreset(
  settings: SessionSettings,
  presets: readonly SessionPreset[],
): SessionPreset | null {
  if (settings.preset === undefined) return null;
  return presets.find((p) => p.id === settings.preset) ?? null;
}

/**
 * `settings` with `key` set to `value`, or left out when `value` is
 * `undefined`: a field left out is inherited again.
 */
export function withSetting<K extends keyof SessionSettings>(
  settings: SessionSettings,
  key: K,
  value: SessionSettings[K] | undefined,
): SessionSettings {
  const next: SessionSettings = { ...settings };
  if (value === undefined) delete next[key];
  else next[key] = value;
  return next;
}

/** The value in force for `field`: the conversation's, else its preset's. */
export function effectiveValue(
  settings: SessionSettings,
  preset: SessionPreset | null,
  field: PresetField,
): number | undefined {
  return settings[field] ?? preset?.request?.[field];
}

/**
 * The settings the conversation changed over its preset: a value of its own
 * that differs from the preset's, or any value of its own without a preset.
 */
export function modifiedFields(
  settings: SessionSettings,
  preset: SessionPreset | null,
): PresetField[] {
  return PRESET_FIELDS.filter((field) => {
    const own = settings[field];
    return own !== undefined && own !== preset?.request?.[field];
  });
}

/** Back on the chosen preset: every modification of the conversation dropped. */
export function discardModifications(settings: SessionSettings): SessionSettings {
  const next: SessionSettings = { ...settings };
  for (const field of PRESET_FIELDS) delete next[field];
  return next;
}

/** One setting back on the chosen preset. */
export function revertField(settings: SessionSettings, field: PresetField): SessionSettings {
  return withSetting(settings, field, undefined);
}

/**
 * The conversation on the preset `id` (`null`: none, the agent's applies):
 * what it had changed over the previous one is dropped with it.
 */
export function choosePreset(settings: SessionSettings, id: number | null): SessionSettings {
  return withSetting(discardModifications(settings), "preset", id ?? undefined);
}

/**
 * The preset that keeps the settings of the conversation under `name`: the
 * request settings of its preset, with its own values over them. The load
 * settings follow the preset; the instructions are never part of it.
 */
export function presetDraftFrom(
  settings: SessionSettings,
  preset: SessionPreset | null,
  name: string,
  model: SessionModel,
): PresetDraft {
  const request: PresetRequest = { ...(preset?.request ?? {}) };
  for (const field of PRESET_FIELDS) {
    const own = settings[field];
    if (own !== undefined) request[field] = own;
  }
  return {
    name: name.trim(),
    model_source: model.model_source,
    model_id: model.model_id,
    load: { ...(preset?.load ?? {}) },
    request,
  };
}

/** A preset with no setting of its own, to start from the defaults of the model. */
export function emptyPresetDraft(name: string, model: SessionModel): PresetDraft {
  return {
    name: name.trim(),
    model_source: model.model_source,
    model_id: model.model_id,
    load: {},
    request: {},
  };
}

/** Why a preset file is not sent to the runtime. */
export type PresetFileError = "not_json" | "not_a_preset";

/**
 * The preset an imported file describes, created for the model of the
 * conversation whatever model the file names; its load settings are kept.
 * The runtime validates the settings themselves when it creates the preset.
 */
export function parsePresetFile(
  text: string,
  model: SessionModel,
): { ok: true; draft: PresetDraft } | { ok: false; error: PresetFileError } {
  let parsed: unknown;
  try {
    parsed = JSON.parse(text);
  } catch {
    return { ok: false, error: "not_json" };
  }
  if (!isRecord(parsed) || typeof parsed.name !== "string" || parsed.name.trim() === "") {
    return { ok: false, error: "not_a_preset" };
  }
  if (
    (parsed.request !== undefined && !isRecord(parsed.request)) ||
    (parsed.load !== undefined && !isRecord(parsed.load))
  ) {
    return { ok: false, error: "not_a_preset" };
  }
  return {
    ok: true,
    draft: {
      name: parsed.name.trim(),
      model_source: model.model_source,
      model_id: model.model_id,
      load: isRecord(parsed.load) ? parsed.load : {},
      request: isRecord(parsed.request) ? (parsed.request as PresetRequest) : {},
    },
  };
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

/** Whether instructions of this length are refused. */
export function instructionsTooLong(text: string): boolean {
  return text.length > INSTRUCTIONS_MAX_CHARS;
}

/** What the conversation stores for the text of its instructions field. */
export function instructionsValue(text: string): string | undefined {
  const trimmed = text.trim();
  return trimmed === "" ? undefined : trimmed;
}

/** A temperature typed or dragged, held between 0 and 2; `undefined` for none. */
export function temperatureValue(raw: string): number | undefined {
  if (raw.trim() === "") return undefined;
  const value = Number(raw);
  if (!Number.isFinite(value)) return undefined;
  return Math.round(Math.min(2, Math.max(0, value)) * 100) / 100;
}

/** An answer length typed, at least one token; `undefined` for none. */
export function maxTokensValue(raw: string): number | undefined {
  if (raw.trim() === "") return undefined;
  const value = Math.floor(Number(raw));
  return Number.isFinite(value) && value >= 1 ? value : undefined;
}

/** The mission an agent gets when the instructions are added at its end. */
export function missionWithInstructions(mission: string, instructions: string): string {
  const added = instructions.trim();
  if (added === "") return mission;
  const base = mission.trimEnd();
  return base === "" ? added : `${base}\n\n${added}`;
}

/** One write of "Apply to the agent", in the order it is sent. */
export type ApplyStep =
  | { kind: "create_preset"; draft: PresetDraft }
  | { kind: "set_model"; model: SessionModel; preset: number | "created" | null }
  | { kind: "set_mission"; mission: string };

/** What "Apply to the agent" takes from the conversation. */
export interface ApplyInput {
  settings: SessionSettings;
  preset: SessionPreset | null;
  model: SessionModel;
  /** Name of the preset created when the conversation changed its settings. */
  presetName: string;
  /** The mission of the agent in force. */
  mission: string;
  /** Whether the instructions are added at the end of the mission. */
  addInstructions: boolean;
}

/**
 * The writes of "Apply to the agent": the agent receives the model of the
 * conversation and its preset. Settings changed over the preset are kept as
 * a new preset first; the instructions reach the mission only when asked.
 */
export function applyToAgentSteps(input: ApplyInput): ApplyStep[] {
  const steps: ApplyStep[] = [];
  const changed = modifiedFields(input.settings, input.preset).length > 0;
  if (changed) {
    steps.push({
      kind: "create_preset",
      draft: presetDraftFrom(input.settings, input.preset, input.presetName, input.model),
    });
  }
  steps.push({
    kind: "set_model",
    model: input.model,
    preset: changed ? "created" : (input.preset?.id ?? null),
  });
  const instructions = input.settings.instructions ?? "";
  if (input.addInstructions && instructions.trim() !== "") {
    steps.push({
      kind: "set_mission",
      mission: missionWithInstructions(input.mission, instructions),
    });
  }
  return steps;
}

/** What the writes of "Apply to the agent" need from the runtime. */
export interface ApplyEffects {
  createPreset: (draft: PresetDraft) => Promise<{ id: number }>;
  setModel: (agent: string, model: SessionModel, preset: number | null) => Promise<void>;
  setMission: (agent: string, mission: string) => Promise<void>;
}

/**
 * Sends `steps` for `agent`, in order, and stops at the first refusal, which
 * it throws: a preset that was not created is never assigned.
 */
export async function runApplySteps(
  agent: string,
  steps: readonly ApplyStep[],
  effects: ApplyEffects,
): Promise<void> {
  let created: number | null = null;
  for (const step of steps) {
    switch (step.kind) {
      case "create_preset":
        created = (await effects.createPreset(step.draft)).id;
        break;
      case "set_model":
        await effects.setModel(agent, step.model, step.preset === "created" ? created : step.preset);
        break;
      case "set_mission":
        await effects.setMission(agent, step.mission);
        break;
    }
  }
}

/** The identifier of the agent a conversation talks to. */
export function agentIdOf(
  session: { mode: "libre" | "agent"; agent_name: string | null },
  builtinId: string,
): string {
  return session.mode === "agent" && session.agent_name ? session.agent_name : builtinId;
}

/** The tabs of the right panel of the chat that hold a conversation's choices. */
export const SESSION_PANEL_TABS = ["integrations", "settings"] as const;
export type SessionPanelTab = (typeof SESSION_PANEL_TABS)[number];

/** The label of each context-limit value, and the sentence that explains it. */
export const CONTEXT_OVERFLOW_KEYS: Record<ContextOverflow, { label: string; help: string }> = {
  summarize_middle: {
    label: "chat.session_settings.response.overflow_summarize_middle",
    help: "chat.session_settings.response.overflow_summarize_middle_help",
  },
  sliding_window: {
    label: "chat.session_settings.response.overflow_sliding_window",
    help: "chat.session_settings.response.overflow_sliding_window_help",
  },
  stop_at_limit: {
    label: "chat.session_settings.response.overflow_stop_at_limit",
    help: "chat.session_settings.response.overflow_stop_at_limit_help",
  },
};

/** The label of each reasoning level. */
export const REASONING_EFFORT_KEYS: Record<ReasoningEffort, string> = {
  low: "chat.session_settings.reasoning.effort_low",
  medium: "chat.session_settings.reasoning.effort_medium",
  high: "chat.session_settings.reasoning.effort_high",
};

/** The name of each setting a preset carries, as the preset block lists them. */
export const PRESET_FIELD_KEYS: Record<PresetField, string> = {
  temperature: "chat.session_settings.preset.field_temperature",
  max_tokens: "chat.session_settings.preset.field_max_tokens",
};
