// `apollia.toml` refused at startup: the runtime of the desktop runs on the
// default values instead, and the application says so in a persistent banner,
// visible with the developer mode off as well, never in silence.
//
// The state is read once the runtime answers: a file whose read carries
// faults at launch was refused at startup. The `ConfigInvalid` event the
// runtime publishes when it is up sets it too, when it arrives before that
// read; a later `ConfigReloaded` says a valid file is now applied and clears it.

import { get, writable } from "svelte/store";
import type { ConfigDiagnostic, ConfigFileView } from "$lib/ipc/configFile";

/** The refused file, with its faults. */
export interface ConfigInvalidState {
  path: string;
  diagnostics: ConfigDiagnostic[];
}

/** Set while the file read at startup is refused; null otherwise. */
export const configInvalidAtStart = writable<ConfigInvalidState | null>(null);

/** The line the editor opens on, consumed once by the editor. */
export const configEditorFocusLine = writable<number | null>(null);

let startupChecked = false;

/** The first faulty line, 1 when the faults name none. */
export function firstFaultLine(state: ConfigInvalidState): number {
  const lines = state.diagnostics.map((d) => d.line).filter((line) => line > 0);
  return lines.length > 0 ? Math.min(...lines) : 1;
}

/** How many tries, and how far apart, while the runtime is still starting. */
export const STARTUP_TRIES = 5;
export const STARTUP_RETRY_MS = 2000;

/**
 * Reads the file once the runtime answers and sets the banner when the read
 * carries faults. Tries a few times while the runtime is still starting.
 */
export async function checkConfigAtStartup(
  read: () => Promise<ConfigFileView>,
  wait: (ms: number) => Promise<void> = (ms) => new Promise((r) => setTimeout(r, ms)),
): Promise<void> {
  for (let attempt = 0; attempt < STARTUP_TRIES; attempt++) {
    try {
      const view = await read();
      startupChecked = true;
      if (view.diagnostics.length > 0) {
        configInvalidAtStart.set({ path: view.path, diagnostics: view.diagnostics });
      }
      return;
    } catch {
      if (attempt < STARTUP_TRIES - 1) await wait(STARTUP_RETRY_MS);
    }
  }
  startupChecked = true;
}

/** Feeds a `runtime-event` of the configuration to the banner. */
export function onConfigEvent(eventType: string, payload: unknown): void {
  if (eventType === "ConfigReloaded") {
    configInvalidAtStart.set(null);
    return;
  }
  if (eventType !== "ConfigInvalid" || startupChecked) return;
  const inner = (payload as { ConfigInvalid?: ConfigInvalidState } | null)?.ConfigInvalid;
  if (inner) configInvalidAtStart.set({ path: inner.path, diagnostics: inner.diagnostics });
}

/** What opening the editor from the banner needs. */
export interface OpenEditorDeps {
  developerMode: () => boolean;
  /** Turns the developer mode on; false when refused (imposed by the policy). */
  turnDeveloperModeOn: () => Promise<boolean>;
  openSettings: (section: string) => void;
}

/**
 * Opens the editor at the faulty line, after turning the developer mode on
 * when it is off (the user confirmed it first). False when the mode could not
 * be turned on.
 */
export async function openEditorAtFault(
  state: ConfigInvalidState,
  deps: OpenEditorDeps,
): Promise<boolean> {
  if (!deps.developerMode() && !(await deps.turnDeveloperModeOn())) return false;
  configEditorFocusLine.set(firstFaultLine(state));
  deps.openSettings("configuration");
  return true;
}

/** Takes the line to open on, if any, and clears it. */
export function consumeFocusLine(): number | null {
  const line = get(configEditorFocusLine);
  if (line !== null) configEditorFocusLine.set(null);
  return line;
}

/** Forgets the startup check, for the tests. */
export function resetConfigInvalidForTests(): void {
  startupChecked = false;
  configInvalidAtStart.set(null);
  configEditorFocusLine.set(null);
}
