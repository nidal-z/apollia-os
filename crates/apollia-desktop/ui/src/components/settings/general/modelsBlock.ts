// The logic of the models block of the General settings, kept out of the
// component so Vitest can assert on it (no DOM in the unit environment).

import type { ResolvedSetting, SettingSource } from "$lib/ipc/settings";

/** The dotted key of the models folder in `apollia.toml`. */
export const MODELS_DIR_KEY = "models.dir";

/** Where a reader creates a Hugging Face token. */
export const HF_TOKENS_URL = "https://huggingface.co/settings/tokens";

/** The state of the token field for what is typed and what is stored. */
export interface TokenFieldState {
  /** The field differs from the stored token: "typed, not yet saved". */
  dirty: boolean;
  /** "Save" is offered: something non-empty and new is typed. */
  canSave: boolean;
  /** "Clear" is offered: a token is stored. */
  canClear: boolean;
}

export function tokenFieldState(typed: string, stored: string): TokenFieldState {
  const value = typed.trim();
  const dirty = value !== stored;
  return { dirty, canSave: dirty && value.length > 0, canClear: stored.length > 0 };
}

/** The folder a directory picker answered, or `null` when it was dismissed. */
export function pickedFolder(selected: unknown): string | null {
  if (typeof selected === "string") return selected.length > 0 ? selected : null;
  if (typeof selected === "object" && selected !== null && "path" in selected) {
    const path = (selected as { path: unknown }).path;
    return typeof path === "string" && path.length > 0 ? path : null;
  }
  return null;
}

/** Whether the organization policy locks the models folder. */
export function folderLocked(source: SettingSource): boolean {
  return source === "policy";
}

/** The shell calls the block makes, replaceable in tests. */
export interface ModelsFolderIpc {
  writeSetting: (key: string, value: unknown) => Promise<ResolvedSetting>;
  readFolder: () => Promise<string>;
}

/**
 * Writes `folder` as the models folder and answers the folder now in force.
 * No model is moved: the next download lands in the new folder, the models
 * already downloaded stay where they are.
 */
export async function changeModelsDir(folder: string, ipc: ModelsFolderIpc): Promise<string> {
  await ipc.writeSetting(MODELS_DIR_KEY, folder);
  return ipc.readFolder();
}
