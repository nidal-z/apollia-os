/**
 * Install preferences of the agent packages.
 *
 * One field for now: `autoInstallPythonDeps`.
 * When that flag is true, the install modal passes `depsConfirmed=true` to
 * the backend automatically, without asking the user for an explicit
 * confirmation on every package that declares pip dependencies.
 *
 * Default: `false` (safe by default - every install that touches pip has to
 * be consented to explicitly).
 *
 * Persisted in localStorage (the pattern `mode.ts` follows). `apollia.toml`
 * stays read-only from the UI, so a preference of this kind is not written
 * there.
 */
import { writable } from "svelte/store";

const STORAGE_KEY = "apollia.agent-install-prefs";

export interface AgentInstallPrefs {
  /** Install the Python dependencies automatically, with no confirmation dialog. */
  autoInstallPythonDeps: boolean;
}

const DEFAULT_PREFS: AgentInstallPrefs = {
  autoInstallPythonDeps: false,
};

function loadPersisted(): AgentInstallPrefs {
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (!stored) return DEFAULT_PREFS;
    const parsed = JSON.parse(stored);
    return {
      autoInstallPythonDeps:
        typeof parsed?.autoInstallPythonDeps === "boolean"
          ? parsed.autoInstallPythonDeps
          : DEFAULT_PREFS.autoInstallPythonDeps,
    };
  } catch {
    return DEFAULT_PREFS;
  }
}

/**
 * Install preferences. Updates are persisted in localStorage.
 * Read the current value through `get(agentInstallPrefs)` (svelte/store).
 */
export const agentInstallPrefs = writable<AgentInstallPrefs>(loadPersisted());

agentInstallPrefs.subscribe((value) => {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(value));
  } catch {
    // localStorage unavailable - ignore silently.
  }
});

/** Helper: flip a boolean field and persist. */
export function setAutoInstallPythonDeps(enabled: boolean): void {
  agentInstallPrefs.update((prefs) => ({ ...prefs, autoInstallPythonDeps: enabled }));
}
