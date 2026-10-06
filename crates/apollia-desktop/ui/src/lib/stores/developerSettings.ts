// The settings Settings > Developer adds under "Engines" and "Experimental".
//
// Each is one key of `apollia.toml`, read and written through the shell, so
// the organization policy can impose any of them and the Rust readers see the
// same value. Two of them change what the application shows outside the
// settings (the debugging blocks of a conversation, the consumption widget of
// the sidebar): the store is seeded at launch, and both are read through the
// derived stores below, which hold them off while the developer mode is off.

import { derived, get, writable } from "svelte/store";
import {
  getResolvedSetting,
  setSetting,
  type ResolvedSetting,
  type SettingSource,
} from "$lib/ipc/settings";
import { developerMode } from "$lib/stores/developerMode";

/** The dotted keys, one per setting, never repeated elsewhere. */
export const DEVELOPER_SETTING_KEYS = {
  enginesAutoPurge: "engines.auto_purge",
  chatDebugBlocks: "desktop.chat_debug_blocks",
  resourceWidget: "desktop.resource_widget",
  presetLoadSettings: "models.preset_load_settings",
} as const;

/** One of the keys above. */
export type DeveloperSettingKey =
  (typeof DEVELOPER_SETTING_KEYS)[keyof typeof DEVELOPER_SETTING_KEYS];

/** The value of each setting when no one wrote one, as Rust defaults it. */
export const DEVELOPER_SETTING_DEFAULTS: Record<DeveloperSettingKey, boolean> = {
  "engines.auto_purge": false,
  "desktop.chat_debug_blocks": true,
  "desktop.resource_widget": false,
  "models.preset_load_settings": false,
};

/** A setting in force and where its value comes from. */
export interface DeveloperSetting {
  value: boolean;
  source: SettingSource;
}

const ALL_KEYS = Object.values(DEVELOPER_SETTING_KEYS) as DeveloperSettingKey[];

function defaults(): Record<DeveloperSettingKey, DeveloperSetting> {
  const entries = ALL_KEYS.map((key) => [
    key,
    { value: DEVELOPER_SETTING_DEFAULTS[key], source: "default" },
  ]);
  return Object.fromEntries(entries) as Record<DeveloperSettingKey, DeveloperSetting>;
}

/** The settings in force, the defaults until the shell answers. */
export const developerSettings = writable<Record<DeveloperSettingKey, DeveloperSetting>>(
  defaults(),
);

/** The two shell calls the store makes, replaceable in tests. */
export interface DeveloperSettingsIpc {
  read: (key: string) => Promise<ResolvedSetting>;
  write: (key: string, value: unknown) => Promise<ResolvedSetting>;
}

const shellIpc: DeveloperSettingsIpc = { read: getResolvedSetting, write: setSetting };

function apply(key: DeveloperSettingKey, resolved: unknown): void {
  if (typeof resolved !== "object" || resolved === null) return;
  const { value, source } = resolved as Partial<ResolvedSetting>;
  developerSettings.update((all) => ({
    ...all,
    [key]: {
      value: typeof value === "boolean" ? value : DEVELOPER_SETTING_DEFAULTS[key],
      source: source ?? all[key].source,
    },
  }));
}

/** Seeds every setting from the value in force. A failure keeps its default. */
export async function hydrateDeveloperSettings(
  ipc: DeveloperSettingsIpc = shellIpc,
): Promise<void> {
  await Promise.all(
    ALL_KEYS.map(async (key) => {
      try {
        apply(key, await ipc.read(key));
      } catch {
        // Shell unreachable: this setting stays at its default.
      }
    }),
  );
}

/**
 * Writes one setting. The switch moves at once and comes back when the shell
 * refuses the write (a value the policy imposes, an unreadable file).
 * Resolves to whether the write went through.
 */
export async function setDeveloperSetting(
  key: DeveloperSettingKey,
  value: boolean,
  ipc: DeveloperSettingsIpc = shellIpc,
): Promise<boolean> {
  const previous = get(developerSettings)[key];
  if (previous.source === "policy") return false;
  developerSettings.update((all) => ({ ...all, [key]: { ...all[key], value } }));
  try {
    apply(key, await ipc.write(key, value));
    return true;
  } catch {
    developerSettings.update((all) => ({ ...all, [key]: previous }));
    return false;
  }
}

/**
 * Whether a conversation shows its debugging blocks: the raw data of a tool
 * step and the detailed trace. They exist only with the developer mode on,
 * where the switch, on by default, hides them without leaving the mode.
 */
export function debugBlocksShown(developerModeOn: boolean, switchOn: boolean): boolean {
  return developerModeOn && switchOn;
}

/** {@link debugBlocksShown} for the values in force. */
export const chatDebugBlocks = derived(
  [developerMode, developerSettings],
  ([$mode, $settings]) =>
    debugBlocksShown($mode, $settings[DEVELOPER_SETTING_KEYS.chatDebugBlocks].value),
);

/**
 * Whether the foot of the sidebar shows the consumption widget: an
 * experimental setting of the developer mode, off by default.
 */
export function resourceWidgetShown(developerModeOn: boolean, switchOn: boolean): boolean {
  return developerModeOn && switchOn;
}

/** {@link resourceWidgetShown} for the values in force. */
export const resourceWidgetVisible = derived(
  [developerMode, developerSettings],
  ([$mode, $settings]) =>
    resourceWidgetShown($mode, $settings[DEVELOPER_SETTING_KEYS.resourceWidget].value),
);
