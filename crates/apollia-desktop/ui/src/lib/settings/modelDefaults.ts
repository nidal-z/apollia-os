// Settings > Model defaults: what every local model receives when nothing
// closer to it says otherwise.
//
// Each value is a named setting of `apollia.toml`, read and written through
// the shell, so the organization policy can impose it. The runtime reads the
// context default, the guardrails and the unload delay at every load or
// sweep; the image ceiling is read by the composer when an image is attached;
// the bypass is read by the load window when a load was refused.

import { getResolvedSetting, setSetting, type ResolvedSetting } from "$lib/ipc/settings";
import type { SettingSource } from "$lib/ipc/settings";
import { DEFAULT_IMAGE_MAX_SIDE } from "$lib/chat/imagePrepare";

/** The dotted keys of the screen, as `apollia.toml` names them. */
export const MODEL_DEFAULT_KEYS = {
  imageLimit: "llm.images.limit_side",
  imageMaxSide: "llm.images.max_side_px",
  contextMode: "llm.defaults.context",
  contextTokens: "llm.defaults.context_tokens",
  guardrailLevel: "llm.guardrails.level",
  guardrailCeiling: "llm.guardrails.ceiling_gb",
  bypass: "llm.guardrails.bypass",
  idleUnload: "llm.residency.idle_unload_minutes",
} as const;

/** One value of the screen. */
export type ModelDefaultField = keyof typeof MODEL_DEFAULT_KEYS;

/** The blocks of the screen, in the order it shows them. */
export const MODEL_DEFAULT_BLOCKS = ["images", "context", "guardrails", "bypass", "unload"] as const;

/** A block of the screen. */
export type ModelDefaultBlock = (typeof MODEL_DEFAULT_BLOCKS)[number];

/** The fields a block holds; a policy that fixes one of them locks the block. */
export const BLOCK_FIELDS: Record<ModelDefaultBlock, readonly ModelDefaultField[]> = {
  images: ["imageLimit", "imageMaxSide"],
  context: ["contextMode", "contextTokens"],
  guardrails: ["guardrailLevel", "guardrailCeiling"],
  bypass: ["bypass"],
  unload: ["idleUnload"],
};

/** A choice of a block: its value, its label and the sentence that explains it. */
export interface ModelDefaultChoice<V extends string> {
  value: V;
  labelKey: string;
  hintKey: string;
  testid: string;
}

/** How the default context window is chosen. */
export type ContextMode = "custom" | "model_max";

/** The two choices of the default context, in reading order. */
export const CONTEXT_CHOICES: readonly ModelDefaultChoice<ContextMode>[] = [
  {
    value: "custom",
    labelKey: "settings.model_defaults.context.custom",
    hintKey: "settings.model_defaults.context.custom_hint",
    testid: "model-defaults-context-custom",
  },
  {
    value: "model_max",
    labelKey: "settings.model_defaults.context.model_max",
    hintKey: "settings.model_defaults.context.model_max_hint",
    testid: "model-defaults-context-model-max",
  },
];

/** A level of the load guardrail, as the runtime names it. */
export type GuardrailLevel = "disabled" | "lenient" | "balanced" | "strict" | "ceiling";

/** The level in force when no one chose one. */
export const DEFAULT_GUARDRAIL_LEVEL: GuardrailLevel = "balanced";

/** The five levels, from the loosest to the ceiling in gigabytes. */
export const GUARDRAIL_CHOICES: readonly ModelDefaultChoice<GuardrailLevel>[] = [
  {
    value: "disabled",
    labelKey: "settings.model_defaults.guardrails.disabled",
    hintKey: "settings.model_defaults.guardrails.disabled_hint",
    testid: "model-defaults-guardrail-disabled",
  },
  {
    value: "lenient",
    labelKey: "settings.model_defaults.guardrails.lenient",
    hintKey: "settings.model_defaults.guardrails.lenient_hint",
    testid: "model-defaults-guardrail-lenient",
  },
  {
    value: "balanced",
    labelKey: "settings.model_defaults.guardrails.balanced",
    hintKey: "settings.model_defaults.guardrails.balanced_hint",
    testid: "model-defaults-guardrail-balanced",
  },
  {
    value: "strict",
    labelKey: "settings.model_defaults.guardrails.strict",
    hintKey: "settings.model_defaults.guardrails.strict_hint",
    testid: "model-defaults-guardrail-strict",
  },
  {
    value: "ceiling",
    labelKey: "settings.model_defaults.guardrails.ceiling",
    hintKey: "settings.model_defaults.guardrails.ceiling_hint",
    testid: "model-defaults-guardrail-ceiling",
  },
];

/** What "load anyway" asks before it goes past a refusal. */
export type LoadBypass = "confirm" | "unrestricted";

/** The bypass in force when no one chose one. */
export const DEFAULT_LOAD_BYPASS: LoadBypass = "confirm";

/** The two choices of the bypass, the default first. */
export const BYPASS_CHOICES: readonly ModelDefaultChoice<LoadBypass>[] = [
  {
    value: "confirm",
    labelKey: "settings.model_defaults.bypass.confirm",
    hintKey: "settings.model_defaults.bypass.confirm_hint",
    testid: "model-defaults-bypass-confirm",
  },
  {
    value: "unrestricted",
    labelKey: "settings.model_defaults.bypass.unrestricted",
    hintKey: "settings.model_defaults.bypass.unrestricted_hint",
    testid: "model-defaults-bypass-unrestricted",
  },
];

/** Whether an unused model is unloaded after a delay, or never. */
export type UnloadMode = "after" | "never";

/** The two choices of the unload delay. */
export const UNLOAD_CHOICES: readonly ModelDefaultChoice<UnloadMode>[] = [
  {
    value: "after",
    labelKey: "settings.model_defaults.unload.after",
    hintKey: "settings.model_defaults.unload.after_hint",
    testid: "model-defaults-unload-after",
  },
  {
    value: "never",
    labelKey: "settings.model_defaults.unload.never",
    hintKey: "settings.model_defaults.unload.never_hint",
    testid: "model-defaults-unload-never",
  },
];

export { DEFAULT_IMAGE_MAX_SIDE };

/** Minutes an unused model stays loaded when no one chose a delay. */
export const DEFAULT_IDLE_UNLOAD_MINUTES = 60;

/** The inclusive bounds of each figure of the screen. */
export const MODEL_DEFAULT_BOUNDS = {
  imageMaxSide: { min: 256, max: 8192 },
  contextTokens: { min: 512, max: 262_144 },
  guardrailCeiling: { min: 1, max: 4096 },
  idleUnload: { min: 1, max: 10_080 },
} as const;

/** A figure of the screen that has bounds. */
export type BoundedField = keyof typeof MODEL_DEFAULT_BOUNDS;

/** Every value of the screen, as the screen shows it. */
export interface ModelDefaultsValues {
  /** Whether an image is held to a largest side. */
  imageLimit: boolean;
  imageMaxSide: number;
  contextMode: ContextMode;
  /** `0` when no one chose a figure: the engine sizes the window. */
  contextTokens: number;
  guardrailLevel: GuardrailLevel;
  /** `0` when no ceiling was written. */
  guardrailCeiling: number;
  bypass: LoadBypass;
  /** `0` never unloads. */
  idleUnload: number;
}

/** The values a machine nobody configured shows. */
export const MODEL_DEFAULTS_INITIAL: ModelDefaultsValues = {
  imageLimit: true,
  imageMaxSide: DEFAULT_IMAGE_MAX_SIDE,
  contextMode: "custom",
  contextTokens: 0,
  guardrailLevel: DEFAULT_GUARDRAIL_LEVEL,
  guardrailCeiling: 0,
  bypass: DEFAULT_LOAD_BYPASS,
  idleUnload: DEFAULT_IDLE_UNLOAD_MINUTES,
};

/** Where each value comes from. */
export type ModelDefaultsSources = Record<ModelDefaultField, SettingSource>;

/** The screen as a whole: its values and their sources. */
export interface ModelDefaultsState {
  values: ModelDefaultsValues;
  sources: ModelDefaultsSources;
}

/** The state before the shell answers: every value at its default. */
export function initialModelDefaults(): ModelDefaultsState {
  const sources = {} as ModelDefaultsSources;
  for (const field of Object.keys(MODEL_DEFAULT_KEYS) as ModelDefaultField[]) {
    sources[field] = "default";
  }
  return { values: { ...MODEL_DEFAULTS_INITIAL }, sources };
}

function oneOf<V extends string>(
  choices: readonly ModelDefaultChoice<V>[],
  value: unknown,
  fallback: V,
): V {
  return choices.find((choice) => choice.value === value)?.value ?? fallback;
}

function wholeNumber(value: unknown, fallback: number): number {
  return typeof value === "number" && Number.isFinite(value) && value >= 0 ? value : fallback;
}

/** The value of `field` as the screen holds it; anything unreadable is its default. */
export function readModelDefault<F extends ModelDefaultField>(
  field: F,
  value: unknown,
): ModelDefaultsValues[F] {
  const initial = MODEL_DEFAULTS_INITIAL;
  const read: ModelDefaultsValues = {
    imageLimit: typeof value === "boolean" ? value : initial.imageLimit,
    imageMaxSide: Math.round(wholeNumber(value, initial.imageMaxSide)) || initial.imageMaxSide,
    contextMode: oneOf(CONTEXT_CHOICES, value, initial.contextMode),
    contextTokens: Math.round(wholeNumber(value, initial.contextTokens)),
    guardrailLevel: oneOf(GUARDRAIL_CHOICES, value, initial.guardrailLevel),
    guardrailCeiling: wholeNumber(value, initial.guardrailCeiling),
    bypass: oneOf(BYPASS_CHOICES, value, initial.bypass),
    idleUnload: Math.round(wholeNumber(value, initial.idleUnload)),
  };
  return read[field];
}

/** What a block allows: locked when the policy fixes one of its values. */
export interface BlockLock {
  /** True when no choice of the block can be changed. */
  disabled: boolean;
  /** The catalogue key of the policy mention, `null` when the block is free. */
  policyLabelKey: string | null;
}

/**
 * Whether the organization policy fixes `block`: its choices are then
 * disabled, the imposed value stays shown, and the block names the policy.
 */
export function blockLock(block: ModelDefaultBlock, sources: ModelDefaultsSources): BlockLock {
  const locked = BLOCK_FIELDS[block].some((field) => sources[field] === "policy");
  return { disabled: locked, policyLabelKey: locked ? "settings.policy_locked" : null };
}

/** Whether the unload delay reads as "after a delay" or "never". */
export function unloadMode(minutes: number): UnloadMode {
  return minutes > 0 ? "after" : "never";
}

/** Why a figure typed in a field is refused. */
export type FieldFault = "not_a_number" | "out_of_bounds" | "above_memory";

/** A figure typed in a field, read: the number to write, or why it is refused. */
export type FieldReading =
  | { ok: true; value: number }
  | { ok: false; fault: FieldFault; min: number; max: number };

/**
 * Reads what was typed in the field of `field`. The figure must be a whole
 * number within the bounds of the field (the ceiling in gigabytes takes
 * decimals); a ceiling above the memory of the machine, when it is known, is
 * refused as such, since no model would ever be held back by it.
 */
export function readField(
  field: BoundedField,
  typed: string | number | null | undefined,
  machineMemoryGb: number | null = null,
): FieldReading {
  const { min, max } = MODEL_DEFAULT_BOUNDS[field];
  const text = String(typed ?? "")
    .replace(/[\s  ]/g, "")
    .replace(",", ".");
  const value = Number(text);
  if (text === "" || !Number.isFinite(value)) {
    return { ok: false, fault: "not_a_number", min, max };
  }
  if (field !== "guardrailCeiling" && !Number.isInteger(value)) {
    return { ok: false, fault: "not_a_number", min, max };
  }
  if (value < min || value > max) {
    return { ok: false, fault: "out_of_bounds", min, max };
  }
  if (field === "guardrailCeiling" && machineMemoryGb !== null && value > machineMemoryGb) {
    return { ok: false, fault: "above_memory", min, max: machineMemoryGb };
  }
  return { ok: true, value };
}

/** The catalogue key of the sentence a refused figure shows. */
export function fieldFaultKey(fault: FieldFault): string {
  switch (fault) {
    case "not_a_number":
      return "settings.model_defaults.errors.not_a_number";
    case "out_of_bounds":
      return "settings.model_defaults.errors.out_of_bounds";
    case "above_memory":
      return "settings.model_defaults.errors.above_memory";
  }
}

/**
 * Whether "load anyway" must ask for an explicit confirmation before it goes
 * past a refusal of the guardrail. Only the choice "no restriction" skips it.
 */
export function loadAnywayNeedsConfirmation(bypass: LoadBypass): boolean {
  return bypass !== "unrestricted";
}

/** The largest side an image keeps, `null` when images are not held to one. */
export function imageSideLimit(values: Pick<ModelDefaultsValues, "imageLimit" | "imageMaxSide">): number | null {
  return values.imageLimit ? values.imageMaxSide : null;
}

/** The two shell calls the screen makes, replaceable in tests. */
export interface ModelDefaultsIpc {
  read: (key: string) => Promise<ResolvedSetting>;
  write: (key: string, value: unknown) => Promise<ResolvedSetting>;
}

const shellIpc: ModelDefaultsIpc = { read: getResolvedSetting, write: setSetting };

function applied<F extends ModelDefaultField>(
  state: ModelDefaultsState,
  field: F,
  resolved: unknown,
): ModelDefaultsState {
  if (typeof resolved !== "object" || resolved === null) return state;
  const { value, source } = resolved as Partial<ResolvedSetting>;
  return {
    values: { ...state.values, [field]: readModelDefault(field, value) },
    sources: { ...state.sources, [field]: source ?? state.sources[field] },
  };
}

/**
 * Reads every value of the screen with its source. A value the shell cannot
 * answer keeps its default, so the screen still opens.
 */
export async function loadModelDefaults(ipc: ModelDefaultsIpc = shellIpc): Promise<ModelDefaultsState> {
  let state = initialModelDefaults();
  const fields = Object.keys(MODEL_DEFAULT_KEYS) as ModelDefaultField[];
  const answers = await Promise.allSettled(fields.map((field) => ipc.read(MODEL_DEFAULT_KEYS[field])));
  answers.forEach((answer, index) => {
    if (answer.status === "fulfilled") state = applied(state, fields[index], answer.value);
  });
  return state;
}

/** The outcome of a write: the state to show, and whether it went through. */
export interface ModelDefaultsWrite {
  state: ModelDefaultsState;
  written: boolean;
}

/**
 * Writes `value` under `field` and returns the state to show. A field the
 * policy fixes is not written at all; a write the shell refuses leaves the
 * state as it was.
 */
export async function writeModelDefault<F extends ModelDefaultField>(
  state: ModelDefaultsState,
  field: F,
  value: ModelDefaultsValues[F],
  ipc: ModelDefaultsIpc = shellIpc,
): Promise<ModelDefaultsWrite> {
  if (state.sources[field] === "policy") return { state, written: false };
  try {
    const resolved = await ipc.write(MODEL_DEFAULT_KEYS[field], value);
    return { state: applied(state, field, resolved), written: true };
  } catch {
    return { state, written: false };
  }
}

/**
 * The largest side the composer holds an image to now, `null` when images are
 * not held to one. A shell that does not answer leaves the default ceiling.
 */
export async function readImageSideLimit(ipc: ModelDefaultsIpc = shellIpc): Promise<number | null> {
  const [limit, side] = await Promise.allSettled([
    ipc.read(MODEL_DEFAULT_KEYS.imageLimit),
    ipc.read(MODEL_DEFAULT_KEYS.imageMaxSide),
  ]);
  return imageSideLimit({
    imageLimit: readModelDefault("imageLimit", limit.status === "fulfilled" ? limit.value.value : undefined),
    imageMaxSide: readModelDefault(
      "imageMaxSide",
      side.status === "fulfilled" ? side.value.value : undefined,
    ),
  });
}
