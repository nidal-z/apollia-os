// The engine that serves each model format, and their automatic update.
//
// The three values live in `[engines]` of `apollia.toml` (`gguf`, `mlx`,
// `auto_update`), read and written through the settings commands of the
// shell: nothing is kept in the interface. The choices offered come from the
// engine list of the runtime, never from a list written here.

import { get, writable } from "svelte/store";
import type { EngineEntry, EnginesResponse } from "$lib/ipc/system";
import {
  getResolvedSetting,
  setSetting,
  type ResolvedSetting,
  type SettingSource,
} from "$lib/ipc/settings";

/** A model format an engine serves. */
export type EngineFormat = "gguf" | "mlx";

/** The formats, in the order the page shows them. */
export const ENGINE_FORMATS: readonly EngineFormat[] = ["gguf", "mlx"];

/** What a format can be served by: the automatic choice, the shipped engine, a store variant. */
export type EngineChoice = "auto" | "embedded" | NonNullable<EngineEntry["variant"]>;

/** The setting each format is written under. */
export const ENGINE_FORMAT_KEYS: Record<EngineFormat, string> = {
  gguf: "engines.gguf",
  mlx: "engines.mlx",
};

/** The setting of the automatic update. */
export const ENGINE_AUTO_UPDATE_KEY = "engines.auto_update";

const FORMAT_ENGINES: Record<EngineFormat, EngineEntry["engine"]> = {
  gguf: "llama-server",
  mlx: "vllm-mlx",
};

const CHOICES: readonly EngineChoice[] = ["auto", "embedded", "cuda", "vulkan", "cpu", "metal"];

export function isEngineChoice(value: unknown): value is EngineChoice {
  return (CHOICES as readonly unknown[]).includes(value);
}

/** One engine a format can be served by. */
export interface EngineOption {
  /** The value written to the setting. */
  value: Exclude<EngineChoice, "auto">;
  engine: EngineEntry["engine"];
  variant: EngineEntry["variant"];
  /** The version the engine states; `null` when it says nothing. */
  version: string | null;
  /** True for the engine shipped with the application. */
  embedded: boolean;
}

/**
 * The engines `format` can be served by, from the engine list: the shipped
 * one, then the newest build of each variant the store holds. The list
 * arrives ordered by tag within a variant, so the last build of a variant is
 * its newest.
 */
export function engineOptions(
  response: EnginesResponse | null,
  format: EngineFormat,
): EngineOption[] {
  if (!response) return [];
  const engine = FORMAT_ENGINES[format];
  const own = response.engines.filter((e) => e.engine === engine);
  const options: EngineOption[] = [];
  const shipped = own.find((e) => e.source === "embedded");
  if (shipped) {
    options.push({
      value: "embedded",
      engine,
      variant: shipped.variant,
      version: shipped.version,
      embedded: true,
    });
  }
  const stored = new Map<NonNullable<EngineEntry["variant"]>, EngineEntry>();
  for (const entry of own) {
    if (entry.source !== "embedded" && entry.variant !== null) stored.set(entry.variant, entry);
  }
  for (const [variant, entry] of stored) {
    options.push({ value: variant, engine, variant, version: entry.version, embedded: false });
  }
  return options;
}

/** What the list of a format shows for the choice the settings hold. */
export interface EngineSelection {
  /** The choice in force: the one written, or the automatic one it fell back to. */
  value: EngineChoice;
  /** True when a variant was chosen and this machine no longer holds it. */
  fellBack: boolean;
}

/**
 * The choice in force for `chosen` among `options`. A variant no option
 * carries has been removed: the automatic choice applies, as it does in the
 * runtime, and the page warns.
 */
export function engineSelection(chosen: EngineChoice, options: EngineOption[]): EngineSelection {
  if (chosen === "auto" || options.some((o) => o.value === chosen)) {
    return { value: chosen, fellBack: false };
  }
  return { value: "auto", fellBack: true };
}

/** The engine in service for `format`, or `null` when the runtime starts none. */
export function engineInService(
  response: EnginesResponse | null,
  format: EngineFormat,
): EngineEntry | null {
  return (
    response?.engines.find((e) => e.engine === FORMAT_ENGINES[format] && e.in_service) ?? null
  );
}

/** A value of the settings with where it comes from; `"policy"` locks its control. */
export interface Held<T> {
  value: T;
  source: SettingSource;
}

/** The three engine settings in force. */
export interface EngineSettingsState {
  gguf: Held<EngineChoice>;
  mlx: Held<EngineChoice>;
  autoUpdate: Held<boolean>;
}

/** The defaults of the runtime: automatic engines, kept up to date. */
export const DEFAULT_ENGINE_SETTINGS: EngineSettingsState = {
  gguf: { value: "auto", source: "default" },
  mlx: { value: "auto", source: "default" },
  autoUpdate: { value: true, source: "default" },
};

export const engineSettings = writable<EngineSettingsState>(DEFAULT_ENGINE_SETTINGS);

/** The two shell calls the store makes, replaceable in tests. */
export interface EngineSettingsIpc {
  read: (key: string) => Promise<ResolvedSetting>;
  write: (key: string, value: unknown) => Promise<ResolvedSetting>;
}

const shellIpc: EngineSettingsIpc = { read: getResolvedSetting, write: setSetting };

function held<T>(
  resolved: unknown,
  accepts: (value: unknown) => value is T,
  fallback: Held<T>,
): Held<T> {
  if (typeof resolved !== "object" || resolved === null) return fallback;
  const { value, source } = resolved as Partial<ResolvedSetting>;
  return {
    value: accepts(value) ? value : fallback.value,
    source: source ?? fallback.source,
  };
}

const isBoolean = (value: unknown): value is boolean => typeof value === "boolean";

/** Seeds the store from the values in force. A value that cannot be read keeps its default. */
export async function hydrateEngineSettings(ipc: EngineSettingsIpc = shellIpc): Promise<void> {
  const read = async (key: string): Promise<unknown> => {
    try {
      return await ipc.read(key);
    } catch {
      return null;
    }
  };
  const gguf = await read(ENGINE_FORMAT_KEYS.gguf);
  const mlx = await read(ENGINE_FORMAT_KEYS.mlx);
  const autoUpdate = await read(ENGINE_AUTO_UPDATE_KEY);
  engineSettings.set({
    gguf: held(gguf, isEngineChoice, DEFAULT_ENGINE_SETTINGS.gguf),
    mlx: held(mlx, isEngineChoice, DEFAULT_ENGINE_SETTINGS.mlx),
    autoUpdate: held(autoUpdate, isBoolean, DEFAULT_ENGINE_SETTINGS.autoUpdate),
  });
}

/**
 * Chooses the engine of `format`. The list moves at once and comes back when
 * the shell refuses the write (a value the policy imposes, an unreadable
 * file). Resolves to whether the write went through.
 */
export async function chooseEngine(
  format: EngineFormat,
  choice: EngineChoice,
  ipc: EngineSettingsIpc = shellIpc,
): Promise<boolean> {
  const previous = get(engineSettings);
  engineSettings.set({ ...previous, [format]: { ...previous[format], value: choice } });
  try {
    const written = held(
      await ipc.write(ENGINE_FORMAT_KEYS[format], choice),
      isEngineChoice,
      { value: choice, source: "user" },
    );
    engineSettings.update((state) => ({ ...state, [format]: written }));
    return true;
  } catch {
    engineSettings.set(previous);
    return false;
  }
}

/** Turns the automatic update of the chosen engines on or off; same contract as {@link chooseEngine}. */
export async function setEngineAutoUpdate(
  on: boolean,
  ipc: EngineSettingsIpc = shellIpc,
): Promise<boolean> {
  const previous = get(engineSettings);
  engineSettings.set({ ...previous, autoUpdate: { ...previous.autoUpdate, value: on } });
  try {
    const written = held(await ipc.write(ENGINE_AUTO_UPDATE_KEY, on), isBoolean, {
      value: on,
      source: "user",
    });
    engineSettings.update((state) => ({ ...state, autoUpdate: written }));
    return true;
  } catch {
    engineSettings.set(previous);
    return false;
  }
}
