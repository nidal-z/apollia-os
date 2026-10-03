// The developer mode: one switch, off by default, for the whole application.
//
// Off, every screen shows its default branch (the former Operator views); on,
// the developer branch (the former Builder views). The value lives in
// `[desktop] developer_mode` of `apollia.toml`, read and written through the
// shell, so the organization policy can impose it and Rust readers see it.

import { get, writable } from "svelte/store";
import {
  getResolvedSetting,
  setSetting,
  type ResolvedSetting,
  type SettingSource,
} from "$lib/ipc/settings";
import {
  forgetLegacyChoice,
  readLegacyChoice,
  type LegacyModeStorage,
} from "$lib/stores/developerModeMigration";

/** The dotted key of the setting in `apollia.toml`. */
export const DEVELOPER_MODE_KEY = "desktop.developer_mode";

/** Whether the developer mode is on. False until the shell answers. */
export const developerMode = writable<boolean>(false);

/** Where the value comes from; `"policy"` locks the switch. */
export const developerModeSource = writable<SettingSource>("default");

/** The two shell calls the store makes, replaceable in tests. */
export interface DeveloperModeIpc {
  read: (key: string) => Promise<ResolvedSetting>;
  write: (key: string, value: unknown) => Promise<ResolvedSetting>;
}

const shellIpc: DeveloperModeIpc = { read: getResolvedSetting, write: setSetting };

function browserStorage(): LegacyModeStorage | null {
  try {
    return globalThis.localStorage ?? null;
  } catch {
    return null;
  }
}

function isResolved(value: unknown): value is ResolvedSetting {
  return typeof value === "object" && value !== null && "value" in value && "source" in value;
}

function apply(resolved: unknown): void {
  if (!isResolved(resolved)) return;
  developerMode.set(resolved.value === true);
  developerModeSource.set(resolved.source);
}

/**
 * Seeds the store at launch: migrates a former Builder choice once, then reads
 * the value in force. A failure keeps the mode off.
 */
export async function hydrateDeveloperMode(
  ipc: DeveloperModeIpc = shellIpc,
  storage: LegacyModeStorage | null = browserStorage(),
): Promise<void> {
  const legacy = readLegacyChoice(storage);
  if (legacy === true) {
    try {
      apply(await ipc.write(DEVELOPER_MODE_KEY, true));
      forgetLegacyChoice(storage);
      return;
    } catch {
      // Refused or unreachable: the old key stays for the next launch.
    }
  } else if (legacy === false) {
    forgetLegacyChoice(storage);
  }
  try {
    apply(await ipc.read(DEVELOPER_MODE_KEY));
  } catch {
    // Shell unreachable: the mode stays off.
  }
}

/**
 * Turns the developer mode on or off. The switch moves at once and comes back
 * when the shell refuses the write (a value the policy imposes, an unreadable
 * file). Resolves to whether the write went through.
 */
export async function setDeveloperMode(
  on: boolean,
  ipc: DeveloperModeIpc = shellIpc,
): Promise<boolean> {
  const previous = get(developerMode);
  developerMode.set(on);
  try {
    apply(await ipc.write(DEVELOPER_MODE_KEY, on));
    return true;
  } catch {
    developerMode.set(previous);
    return false;
  }
}

/** Flips the developer mode, for the command palette. */
export function toggleDeveloperMode(): Promise<boolean> {
  return setDeveloperMode(!get(developerMode));
}
