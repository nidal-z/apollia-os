/**
 * UI mode store for Apollia Desktop.
 *
 * Controls whether the UI shows simplified "operator" views or detailed
 * "builder" views with technical information (cron expressions, token
 * counts, memory namespace links, etc.).
 *
 * Persisted in localStorage so the choice survives page reloads.
 */
import { writable } from "svelte/store";

/** Available display modes for the desktop UI. */
export type UIMode = "operator" | "builder";

const STORAGE_KEY = "apollia-ui-mode";
const DEFAULT_MODE: UIMode = "operator";

function loadPersistedMode(): UIMode {
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (stored === "operator" || stored === "builder") {
      return stored;
    }
  } catch {
    // localStorage unavailable - use default
  }
  return DEFAULT_MODE;
}

/** Current UI mode: "operator" (simplified) or "builder" (technical). */
export const uiMode = writable<UIMode>(loadPersistedMode());

// Persist every change to localStorage.
uiMode.subscribe((value) => {
  try {
    localStorage.setItem(STORAGE_KEY, value);
  } catch {
    // localStorage unavailable - silently ignore
  }
});
