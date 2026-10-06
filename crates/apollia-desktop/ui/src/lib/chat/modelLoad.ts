/**
 * The rules of loading a local model by hand: when the loading window opens,
 * what it asks the runtime, what a refusal of the load guardrail shows, and
 * when a model can be ejected or the last one loaded again.
 *
 * Pure functions, plus the two flows that call the runtime, written over the
 * calls they make so a test replaces them.
 */
import type { ChatTarget, ModelTarget } from "$lib/chat/chatTargets";
import type {
  GuardrailLevel,
  LastLoadedModel,
  LoadEstimate,
  LoadedModel,
  LoadedModelsResponse,
  ModelLoadOutcome,
  ModelLoadRequest,
} from "$lib/types";

/** Smallest context window the window lets one ask for. */
export const MIN_CONTEXT = 1024;
/** Step of the context slider, in tokens. */
export const CONTEXT_STEP = 1024;

/** What the loading window holds; `null` means "left as the model has it". */
export interface LoadForm {
  /** Context window in tokens. */
  nCtx: number | null;
  /** Layers placed on the graphics card. */
  nGpuLayers: number | null;
  /** Evaluation batch size (`n_batch`). */
  batch: number | null;
  /** Compute threads (`n_threads`). */
  threads: number | null;
  /** Seed (`seed`). */
  seed: number | null;
  /** Keep the context cache on the graphics card (`kv_offload`). */
  kvOffload: boolean | null;
  /** Write these settings to the model's defaults. */
  remember: boolean;
  /** Keep the model in memory once loaded. */
  pin: boolean;
}

/** A window nobody touched: every setting as the model has it. */
export function emptyForm(pin = false): LoadForm {
  return {
    nCtx: null,
    nGpuLayers: null,
    batch: null,
    threads: null,
    seed: null,
    kvOffload: null,
    remember: false,
    pin,
  };
}

function wholeNumber(value: unknown): number | null {
  return typeof value === "number" && Number.isInteger(value) ? value : null;
}

/** The window filled from load settings recorded by their preset keys. */
export function formFromSettings(load: Record<string, unknown>, pin = false): LoadForm {
  return {
    ...emptyForm(pin),
    nCtx: wholeNumber(load.n_ctx),
    nGpuLayers: wholeNumber(load.n_gpu_layers),
    batch: wholeNumber(load.n_batch),
    threads: wholeNumber(load.n_threads),
    seed: wholeNumber(load.seed),
    kvOffload: typeof load.kv_offload === "boolean" ? load.kv_offload : null,
  };
}

/**
 * The request the window sends for `modelId`. A model that takes no setting
 * (MLX) is asked for alone, and nothing is remembered for it.
 */
export function loadRequest(
  modelId: string,
  form: LoadForm,
  configurable: boolean,
  confirmed = false,
): ModelLoadRequest {
  const request: ModelLoadRequest = { modelId };
  if (configurable) {
    if (form.nCtx !== null) request.nCtx = form.nCtx;
    if (form.nGpuLayers !== null) request.nGpuLayers = form.nGpuLayers;
    const advanced: Record<string, unknown> = {};
    if (form.batch !== null) advanced.n_batch = form.batch;
    if (form.threads !== null) advanced.n_threads = form.threads;
    if (form.seed !== null) advanced.seed = form.seed;
    if (form.kvOffload !== null) advanced.kv_offload = form.kvOffload;
    if (Object.keys(advanced).length > 0) request.advanced = advanced;
    if (form.remember) request.remember = true;
  }
  if (confirmed) request.confirmed = true;
  return request;
}

/** `value` held between `min` and `max`, as a whole number. */
export function clamp(value: number, min: number, max: number): number {
  if (!Number.isFinite(value)) return min;
  return Math.min(Math.max(Math.round(value), min), Math.max(min, max));
}

/** The bounds of the context field and of its slider. */
export interface ContextBounds {
  min: number;
  /** The largest window the model accepts. */
  max: number;
  step: number;
}

/**
 * The bounds of the context: up to the largest window the model accepts,
 * else up to the one it would load with when its header does not say.
 */
export function contextBounds(estimate: LoadEstimate): ContextBounds {
  const max = Math.max(estimate.maxCtx ?? estimate.nCtx, MIN_CONTEXT);
  return { min: Math.min(MIN_CONTEXT, max), max, step: CONTEXT_STEP };
}

/** The context asked, held to the bounds of the model. */
export function clampContext(value: number, estimate: LoadEstimate): number {
  const bounds = contextBounds(estimate);
  return clamp(value, bounds.min, bounds.max);
}

/** The offload asked, held between none and every layer of the model. */
export function clampOffload(value: number, estimate: LoadEstimate): number {
  return clamp(value, 0, estimate.totalLayers ?? Math.max(estimate.nGpuLayers ?? 0, 0));
}

/** The layers shown on the graphics card: every layer when the plan says "all". */
export function shownOffload(form: LoadForm, estimate: LoadEstimate): number {
  const total = estimate.totalLayers ?? null;
  const planned = form.nGpuLayers ?? estimate.nGpuLayers ?? 0;
  if (total === null) return Math.max(planned, 0);
  return planned < 0 ? total : Math.min(planned, total);
}

/** What the window shows when the load guardrail refuses the load. */
export interface LoadRefusal {
  /** Memory the load would take, in GB. */
  estimatedGb: number;
  /** Memory the guardrail allows, in GB. */
  budgetGb: number;
  level: GuardrailLevel;
}

/** The refusal an estimate carries, `null` when the load goes ahead. */
export function refusalOf(estimate: LoadEstimate | null): LoadRefusal | null {
  if (!estimate || estimate.guardrail.verdict !== "refused" || !estimate.memory) return null;
  return {
    estimatedGb: estimate.memory.totalGb,
    budgetGb: estimate.guardrail.budgetGb ?? estimate.memory.capacityGb,
    level: estimate.guardrail.level,
  };
}

/** A figure in GB to one decimal, in the words of `locale`. */
export function gb(value: number, locale: string): string {
  return value.toLocaleString(locale, { minimumFractionDigits: 1, maximumFractionDigits: 1 });
}

/** The resident instance of the model at `path`, `null` when it is not loaded. */
export function loadedInstance(
  loaded: LoadedModelsResponse | null,
  path: string | null,
): LoadedModel | null {
  if (!loaded || path === null) return null;
  return loaded.models.find((m) => m.modelPath === path) ?? null;
}

/** Whether `target` is a model of this computer. */
export function isLocalModel(target: ChatTarget): target is ModelTarget {
  return target.kind === "model" && target.source === "local";
}

/**
 * Whether choosing `target` opens the loading window first: a model of this
 * computer that is not in memory. A model of a server loads nothing here,
 * and an agent is chosen with its model. While the resident models are not
 * known, the window opens: it is the place that says what a load takes.
 */
export function needsLoadWindow(
  target: ChatTarget,
  loaded: LoadedModelsResponse | null,
): target is ModelTarget {
  if (!isLocalModel(target)) return false;
  if (loaded !== null && !loaded.engineAvailable) return false;
  return loadedInstance(loaded, target.path) === null;
}

/** Whether a model answering the chat is in memory. */
export function hasChatModelLoaded(loaded: LoadedModelsResponse | null): boolean {
  return (loaded?.models ?? []).some((m) => m.kind === "chat");
}

/**
 * Whether "Reload the last model" shows: this computer runs models itself,
 * none answering the chat is in memory, and one was loaded by hand before.
 */
export function showReloadLast(
  loaded: LoadedModelsResponse | null,
  last: LastLoadedModel | null,
): boolean {
  return loaded !== null && loaded.engineAvailable && last !== null && !hasChatModelLoaded(loaded);
}

/** Whether a model can be ejected: it is in memory and answers nothing. */
export function canEject(model: LoadedModel | null): boolean {
  return model !== null && model.activeRequests === 0;
}

/** The calls a load makes, replaceable in a test. */
export interface LoadCalls {
  load: (request: ModelLoadRequest) => Promise<ModelLoadOutcome>;
  setPin: (modelPath: string, pinned: boolean) => Promise<unknown>;
}

/** What became of an attempt to load. */
export type LoadAttempt =
  /** The guardrail refuses the load: nothing was asked of the runtime. */
  | { kind: "needs_confirmation"; refusal: LoadRefusal }
  | { kind: "loaded"; outcome: ModelLoadOutcome }
  | { kind: "failed"; message: string };

/**
 * Loads the model of the window. A load the guardrail refuses is not sent
 * until `confirmed` says the user confirmed it explicitly; it then carries
 * that consent. Once loaded, the pin follows the choice of the window when
 * it differs from `pinnedBefore`.
 */
export async function attemptLoad(
  calls: LoadCalls,
  modelId: string,
  form: LoadForm,
  estimate: LoadEstimate | null,
  confirmed: boolean,
  pinnedBefore: boolean,
): Promise<LoadAttempt> {
  const refusal = refusalOf(estimate);
  if (refusal !== null && !confirmed) return { kind: "needs_confirmation", refusal };
  const request = loadRequest(modelId, form, estimate?.configurable ?? true, refusal !== null);
  let outcome: ModelLoadOutcome;
  try {
    outcome = await calls.load(request);
  } catch (err: unknown) {
    return { kind: "failed", message: err instanceof Error ? err.message : String(err) };
  }
  if (form.pin !== pinnedBefore) {
    await calls.setPin(outcome.loaded.modelPath, form.pin).catch(() => {
      /* the model is loaded; the pin is set again from the AI screen */
    });
  }
  return { kind: "loaded", outcome };
}

/** What became of "Reload the last model". */
export type ReloadAttempt =
  | { kind: "loaded"; outcome: ModelLoadOutcome }
  /** The reload did not go through: the loading window says why. */
  | { kind: "window"; last: LastLoadedModel; message: string };

/**
 * Loads `last` again under the settings it had. When the runtime refuses,
 * the guardrail among the reasons, the answer is to open the loading window
 * on that model with those settings, where the estimate and the refusal show.
 */
export async function reloadLast(
  reload: (confirmed: boolean) => Promise<ModelLoadOutcome>,
  last: LastLoadedModel,
): Promise<ReloadAttempt> {
  try {
    return { kind: "loaded", outcome: await reload(false) };
  } catch (err: unknown) {
    return { kind: "window", last, message: err instanceof Error ? err.message : String(err) };
  }
}

/** Whether `event` is the reload shortcut: Cmd R on macOS, Ctrl R elsewhere. */
export function isReloadShortcut(
  event: Pick<KeyboardEvent, "code" | "metaKey" | "ctrlKey" | "shiftKey" | "altKey">,
  mac: boolean,
): boolean {
  if (event.code !== "KeyR" || event.shiftKey || event.altKey) return false;
  return mac ? event.metaKey && !event.ctrlKey : event.ctrlKey && !event.metaKey;
}

/** What became of an eject. */
export type EjectAttempt =
  /** No local model of the conversation is in memory, or it is answering. */
  | { kind: "nothing" }
  | { kind: "ejected"; model: LoadedModel }
  | { kind: "failed"; model: LoadedModel; message: string };

/** Unloads the resident model of the conversation on screen. */
export async function ejectModel(
  unload: (modelPath: string) => Promise<unknown>,
  model: LoadedModel | null,
): Promise<EjectAttempt> {
  if (model === null || !canEject(model)) return { kind: "nothing" };
  try {
    await unload(model.modelPath);
    return { kind: "ejected", model };
  } catch (err: unknown) {
    return { kind: "failed", model, message: err instanceof Error ? err.message : String(err) };
  }
}
