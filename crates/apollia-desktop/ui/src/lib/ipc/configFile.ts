// Typed IPC wrappers for `apollia.toml` as the editor of Settings >
// Configuration reads and writes it.
//
// Each command is one call to a configuration route of the runtime the window
// drives (`/api/v1/config`), so the validation, the organization policy and
// the check that the file did not change since it was read are the runtime's.

/** One fault of a configuration text, at its line. */
export interface ConfigDiagnostic {
  /** 1-based line. */
  line: number;
  /** 1-based column. */
  column: number;
  /** Dotted path of the key at fault; empty at the root. */
  path: string;
  /** What is wrong, in English, as the runtime words it. */
  message: string;
}

/** A key the organization policy fixes, with its value. */
export interface LockedConfigKey {
  key: string;
  value: unknown;
}

/** A key changed since the start that waits for a restart, with its line. */
export interface PendingRestartKey {
  key: string;
  /** 1-based line of the key, or null when the file no longer writes it. */
  line: number | null;
}

/** `apollia.toml` as the runtime reads it. */
export interface ConfigFileView {
  path: string;
  exists: boolean;
  text: string;
  /** SHA-256 of `text`, presented again when saving. */
  sha256: string;
  diagnostics: ConfigDiagnostic[];
  locked: LockedConfigKey[];
  pendingRestart: PendingRestartKey[];
}

/** The verdict on a text, without writing it. */
export interface ConfigValidation {
  valid: boolean;
  diagnostics: ConfigDiagnostic[];
}

/** Reads the file, its faults, its fixed keys and its keys waiting for a restart. */
export async function readConfigFile(): Promise<ConfigFileView> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<ConfigFileView>("read_config_file");
}

/** Checks a whole text without writing it. */
export async function validateConfigText(text: string): Promise<ConfigValidation> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<ConfigValidation>("validate_config_text", { text });
}

/**
 * Saves a whole text made from the text whose fingerprint is `sha256`.
 * Rejects when the text is invalid, when a line changes a key the policy
 * fixes, or when the file changed since it was read.
 */
export async function saveConfigText(text: string, sha256: string): Promise<ConfigFileView> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<ConfigFileView>("save_config_text", { text, sha256 });
}

/** The default text, the keys the policy fixes written at their value. */
export async function defaultConfigText(): Promise<string> {
  const { invoke } = await import("@tauri-apps/api/core");
  const answer = await invoke<{ text: string }>("default_config_text");
  return answer.text;
}

/**
 * Restarts Apollia so the keys waiting for a restart take effect: the
 * application relaunches, after stopping the service it is attached to.
 */
export async function restartApollia(): Promise<void> {
  const { invoke } = await import("@tauri-apps/api/core");
  await invoke<void>("restart_apollia");
}
