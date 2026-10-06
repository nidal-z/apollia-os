// The "Engines" and "Experimental" blocks of Settings > Developer: which rows
// they hold, in reading order, and the texts of each.
//
// The decisions live here, apart from the components, so Vitest can hold them
// without rendering anything. The values go through the stores of the keys
// they name: the engine settings System > Engines shows too, and the
// developer settings.

import { DEVELOPER_SETTING_KEYS, type DeveloperSettingKey } from "$lib/stores/developerSettings";

/** One switch of a block: its key and its i18n texts. */
export interface DeveloperSwitchRow {
  key: DeveloperSettingKey;
  label: string;
  hint: string;
  /** The i18n keys of the explanation behind a "?", when the row has one. */
  help?: { text: string; label: string };
}

/** The i18n keys of the engines block. */
export const DEVELOPER_ENGINES_KEYS = {
  title: "settings.developer.engines.title",
  channelLabel: "settings.developer.engines.channel_label",
  channelHint: "settings.developer.engines.channel_hint",
  channelFailed: "settings.developer.engines.channel_failed",
  autoUpdateLabel: "settings.engines.auto_update_label",
  autoUpdateHint: "settings.developer.engines.auto_update_hint",
  writeFailed: "settings.developer.engines.setting_write_failed",
} as const;

/** "Delete the least used engines automatically", the last row of the engines block. */
export const AUTO_PURGE_ROW: DeveloperSwitchRow = {
  key: DEVELOPER_SETTING_KEYS.enginesAutoPurge,
  label: "settings.developer.engines.auto_purge_label",
  hint: "settings.developer.engines.auto_purge_hint",
};

/** The i18n key of the toast shown when an experimental switch is refused. */
export const EXPERIMENTAL_WRITE_FAILED = "settings.developer.experimental.write_failed";

/**
 * The experimental switches of this block: the debugging blocks of a
 * conversation, the consumption widget, the load settings in presets. The
 * section renders them in that order, each under its own test id; the
 * separation of the reasoning, a key of the compatible server, follows them.
 */
export const EXPERIMENTAL_ROWS: readonly [
  DeveloperSwitchRow,
  DeveloperSwitchRow,
  DeveloperSwitchRow,
] = [
  {
    key: DEVELOPER_SETTING_KEYS.chatDebugBlocks,
    label: "settings.developer.experimental.debug_blocks_label",
    hint: "settings.developer.experimental.debug_blocks_hint",
  },
  {
    key: DEVELOPER_SETTING_KEYS.resourceWidget,
    label: "settings.developer.experimental.resource_widget_label",
    hint: "settings.developer.experimental.resource_widget_hint",
  },
  {
    key: DEVELOPER_SETTING_KEYS.presetLoadSettings,
    label: "settings.developer.experimental.preset_load_label",
    hint: "settings.developer.experimental.preset_load_hint",
    help: {
      text: "settings.developer.experimental.preset_load_help",
      label: "settings.developer.experimental.preset_load_help_label",
    },
  },
];
