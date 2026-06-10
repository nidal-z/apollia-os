// Global "always plan" default for new chat sessions.
//
// User-level preference, persisted in localStorage like the other desktop UI
// preferences (theme, ui-mode). A new chat session inherits this value once,
// at first load, by calling `set_plan_mode` through `$lib/ipc/planMode`; the
// per-session header chip then overrides the default for that session without
// changing this global setting.

import { writable } from "svelte/store";

const STORAGE_KEY = "apollia-plan-mode-default";

function loadDefault(): boolean {
  try {
    return localStorage.getItem(STORAGE_KEY) === "true";
  } catch {
    return false;
  }
}

/** Global "always plan" default for new chat sessions. */
export const planModeDefault = writable<boolean>(loadDefault());

planModeDefault.subscribe((value) => {
  try {
    localStorage.setItem(STORAGE_KEY, value ? "true" : "false");
  } catch {
    // localStorage unavailable - silently ignore.
  }
});
