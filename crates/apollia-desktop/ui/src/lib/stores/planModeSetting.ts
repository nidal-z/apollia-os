// Global "always plan" default for new chat sessions.
//
// The runtime is the single source of truth for the default: it reads
// `[chat] plan_mode_default` from `apollia.toml` and applies it when a new
// session is created. This store mirrors that default for the desktop UI so the
// Settings toggle has something to render and so the per-user choice survives
// across restarts.
//
// On boot, `hydratePlanModeDefault()` seeds the store from the backend value
// unless the user has already overridden it locally (localStorage). Writes go
// through localStorage so the override persists. The store is never an
// independent truth: absent a local override, it reflects the backend config.

import { writable } from "svelte/store";
import { getPlanModeDefault } from "$lib/ipc/planMode";

const STORAGE_KEY = "apollia-plan-mode-default";

function readStored(): boolean | null {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw === null) return null;
    return raw === "true";
  } catch {
    return null;
  }
}

function loadInitial(): boolean {
  return readStored() ?? false;
}

/** Global "always plan" default for new chat sessions. */
export const planModeDefault = writable<boolean>(loadInitial());

planModeDefault.subscribe((value) => {
  try {
    localStorage.setItem(STORAGE_KEY, value ? "true" : "false");
  } catch {
    // localStorage unavailable - silently ignore.
  }
});

/**
 * Seeds the store from the backend config default.
 *
 * Called once at app boot. When the user has no local override yet, the backend
 * value (the config single source of truth) becomes the initial store value.
 * When a local override exists, it wins and the backend value is ignored, so the
 * user's per-machine choice is never clobbered. Failures are non-fatal: the
 * store keeps its current value.
 */
export async function hydratePlanModeDefault(): Promise<void> {
  if (readStored() !== null) return;
  try {
    const backendDefault = await getPlanModeDefault();
    planModeDefault.set(backendDefault);
  } catch {
    // Backend unavailable - keep the local default (off).
  }
}
