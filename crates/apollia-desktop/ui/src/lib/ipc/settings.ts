// Typed IPC wrappers for the named settings of `apollia.toml`.
//
// A named setting is a dotted key (`desktop.developer_mode`). The shell
// resolves it with the source of its value: the section's default, the user's
// file, or the organization policy, which locks it.

/** Where the value of a named setting comes from. */
export type SettingSource = "default" | "user" | "policy";

/** A named setting with its value in force and the source of that value. */
export interface ResolvedSetting {
  key: string;
  value: unknown;
  source: SettingSource;
}

/** Reads the named setting `key` with its source. */
export async function getResolvedSetting(key: string): Promise<ResolvedSetting> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<ResolvedSetting>("get_resolved_setting", { key });
}

/**
 * Writes the named setting `key`. Refused when the policy imposes it, or when
 * `value` does not have the type of the setting.
 */
export async function setSetting(key: string, value: unknown): Promise<ResolvedSetting> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<ResolvedSetting>("set_setting", { key, value });
}
