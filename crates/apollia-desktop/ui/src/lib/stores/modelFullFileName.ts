/**
 * Whether the list of the models of this computer names each model by the
 * file of its weights rather than by its display name.
 *
 * Off by default. A display preference: it changes what the list shows and
 * nothing the application does, so it is persisted the way the theme is
 * (`theme.ts`) and never imposed by the organization policy.
 */
import { writable } from "svelte/store";

export const MODEL_FULL_FILE_NAME_DEFAULT = false;

export const MODEL_FULL_FILE_NAME_STORAGE_KEY = "apollia-model-full-file-name";

/** A stored value, or the default when it is missing or unknown. */
export function parseModelFullFileName(raw: string | null): boolean {
  if (raw === "on") return true;
  if (raw === "off") return false;
  return MODEL_FULL_FILE_NAME_DEFAULT;
}

function readStored(): boolean {
  if (typeof localStorage === "undefined") return MODEL_FULL_FILE_NAME_DEFAULT;
  try {
    return parseModelFullFileName(localStorage.getItem(MODEL_FULL_FILE_NAME_STORAGE_KEY));
  } catch {
    return MODEL_FULL_FILE_NAME_DEFAULT;
  }
}

/** Whether the list shows the file name of each model. */
export const modelFullFileName = writable<boolean>(readStored());

modelFullFileName.subscribe((value) => {
  if (typeof localStorage === "undefined") return;
  try {
    localStorage.setItem(MODEL_FULL_FILE_NAME_STORAGE_KEY, value ? "on" : "off");
  } catch {
    /* quota or privacy mode: the choice lives for this session only */
  }
});
