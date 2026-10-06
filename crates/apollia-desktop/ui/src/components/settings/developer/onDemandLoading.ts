// "On-demand loading and idle delay" and the "Experimental" separation of the
// reasoning, in Settings > Developer.
//
// The decisions live here, apart from the components, so Vitest can hold them
// without rendering anything. The values are the keys of `[server]` that the
// server settings show too, through the one store `$lib/stores/serverSettings`.

import { SERVER_SETTING_KEYS, type ServerSettingKey } from "$lib/stores/serverSettings";

/** One row of a block: its key, its i18n texts, its test id. */
export interface SettingRow {
  key: ServerSettingKey;
  label: string;
  hint: string;
  testid: string;
}

/** The i18n keys of the on-demand loading block. */
export const ON_DEMAND_KEYS = {
  title: "settings.developer.on_demand.title",
  shared: "settings.developer.on_demand.shared",
  unit: "settings.developer.on_demand.idle_unit",
  invalid: "settings.developer.on_demand.idle_invalid",
  writeFailed: "settings.developer.on_demand.write_failed",
} as const;

/** The rows of the block, in reading order: the delay goes under auto-unload. */
export const ON_DEMAND_ROWS: readonly SettingRow[] = [
  {
    key: SERVER_SETTING_KEYS.jitLoading,
    label: "settings.developer.on_demand.loading_label",
    hint: "settings.developer.on_demand.loading_hint",
    testid: "developer-jit-loading",
  },
  {
    key: SERVER_SETTING_KEYS.jitKeepLastOnly,
    label: "settings.developer.on_demand.keep_last_label",
    hint: "settings.developer.on_demand.keep_last_hint",
    testid: "developer-jit-keep-last",
  },
  {
    key: SERVER_SETTING_KEYS.jitAutoUnload,
    label: "settings.developer.on_demand.auto_unload_label",
    hint: "settings.developer.on_demand.auto_unload_hint",
    testid: "developer-jit-auto-unload",
  },
  {
    key: SERVER_SETTING_KEYS.jitIdleTtlMinutes,
    label: "settings.developer.on_demand.idle_label",
    hint: "settings.developer.on_demand.idle_hint",
    testid: "developer-jit-idle",
  },
];

/** The i18n keys of the experimental block. */
export const EXPERIMENTAL_KEYS = {
  title: "settings.developer.experimental.title",
  writeFailed: "settings.developer.on_demand.write_failed",
} as const;

/** The row of the separation of the reasoning, in the experimental block. */
export const SEPARATE_REASONING_ROW: SettingRow = {
  key: SERVER_SETTING_KEYS.separateReasoning,
  label: "settings.developer.experimental.separate_reasoning_label",
  hint: "settings.developer.experimental.separate_reasoning_hint",
  testid: "developer-separate-reasoning",
};

/** Both blocks exist only with the developer mode on. */
export function onDemandVisible(developerMode: boolean): boolean {
  return developerMode;
}

/**
 * The delay typed in the field, as whole minutes; `null` for anything else (a
 * blank, a fraction, a negative number). `0` keeps the model loaded.
 */
export function parseIdleMinutes(text: string): number | null {
  const trimmed = text.trim();
  if (!/^\d{1,9}$/.test(trimmed)) return null;
  return Number(trimmed);
}

/** The delay field takes a value only while auto-unload is on. */
export function idleFieldDisabled(autoUnload: boolean, locked: boolean): boolean {
  return locked || !autoUnload;
}
