// The log level of the local engine, in Settings > Developer: debug by
// default, the lowest level at which the engine writes its load summary.
//
// The value lives in `[engines] log_level` of `apollia.toml`, read and written
// through the shell, so the organization policy can impose it; the runtime
// reads it each time a model loads, and `APOLLIA_LLAMA_LOG_VERBOSITY` still
// wins over it.

import { get, writable } from "svelte/store";
import {
  getResolvedSetting,
  setSetting,
  type ResolvedSetting,
  type SettingSource,
} from "$lib/ipc/settings";

/** The dotted key of the setting in `apollia.toml`. */
export const ENGINE_LOG_LEVEL_KEY = "engines.log_level";

/** The levels, from the quietest to the most verbose, as the list offers them. */
export const ENGINE_LOG_LEVELS = ["error", "warning", "info", "debug"] as const;

/** A level the engine knows. */
export type EngineLogLevel = (typeof ENGINE_LOG_LEVELS)[number];

/** The level in force when no one wrote one. */
export const DEFAULT_ENGINE_LOG_LEVEL: EngineLogLevel = "debug";

/** The catalogue key of each level's label; the default's names itself so. */
export const ENGINE_LOG_LEVEL_LABEL_KEYS: Record<EngineLogLevel, string> = {
  error: "settings.developer.engines.level_error",
  warning: "settings.developer.engines.level_warning",
  info: "settings.developer.engines.level_info",
  debug: "settings.developer.engines.level_debug",
};

/** The level in force. Debug until the shell answers. */
export const engineLogLevel = writable<EngineLogLevel>(DEFAULT_ENGINE_LOG_LEVEL);

/** Where the value comes from; `"policy"` locks the list. */
export const engineLogLevelSource = writable<SettingSource>("default");

/** The two shell calls the store makes, replaceable in tests. */
export interface EngineLogLevelIpc {
  read: (key: string) => Promise<ResolvedSetting>;
  write: (key: string, value: unknown) => Promise<ResolvedSetting>;
}

const shellIpc: EngineLogLevelIpc = { read: getResolvedSetting, write: setSetting };

/** Whether `value` names a level; anything else reads as the default. */
export function isEngineLogLevel(value: unknown): value is EngineLogLevel {
  return (ENGINE_LOG_LEVELS as readonly unknown[]).includes(value);
}

function apply(resolved: unknown): void {
  if (typeof resolved !== "object" || resolved === null) return;
  const { value, source } = resolved as Partial<ResolvedSetting>;
  engineLogLevel.set(isEngineLogLevel(value) ? value : DEFAULT_ENGINE_LOG_LEVEL);
  if (source) engineLogLevelSource.set(source);
}

/** Seeds the store from the value in force. A failure keeps the default. */
export async function hydrateEngineLogLevel(ipc: EngineLogLevelIpc = shellIpc): Promise<void> {
  try {
    apply(await ipc.read(ENGINE_LOG_LEVEL_KEY));
  } catch {
    // Shell unreachable: the level stays at its default.
  }
}

/**
 * Chooses the engine log level. The list moves at once and comes back when the
 * shell refuses the write (a value the policy imposes, an unreadable file).
 * Resolves to whether the write went through.
 */
export async function setEngineLogLevel(
  level: EngineLogLevel,
  ipc: EngineLogLevelIpc = shellIpc,
): Promise<boolean> {
  const previous = get(engineLogLevel);
  engineLogLevel.set(level);
  try {
    apply(await ipc.write(ENGINE_LOG_LEVEL_KEY, level));
    return true;
  } catch {
    engineLogLevel.set(previous);
    return false;
  }
}
