// The behaviours Settings > General sets and the organization policy can
// impose: whether choosing a model that is not in memory opens the loading
// window, and whether saving into a preset asks for a confirmation.
//
// Each is one key of `apollia.toml`, read and written through the shell. They
// decide what the interface does, so the store is seeded at launch.

import { derived, get, writable, type Readable } from "svelte/store";
import {
  getResolvedSetting,
  setSetting,
  type ResolvedSetting,
  type SettingSource,
} from "$lib/ipc/settings";

/** The dotted keys, one per behaviour, never repeated elsewhere. */
export const GENERAL_BEHAVIOUR_KEYS = {
  modelLoadWindow: "desktop.model_load_window",
  presetSaveConfirm: "desktop.preset_save_confirm",
} as const;

/** One of the keys above. */
export type GeneralBehaviourKey =
  (typeof GENERAL_BEHAVIOUR_KEYS)[keyof typeof GENERAL_BEHAVIOUR_KEYS];

/** The value of each behaviour when no one wrote one, as Rust defaults it. */
export const GENERAL_BEHAVIOUR_DEFAULTS: Record<GeneralBehaviourKey, boolean> = {
  "desktop.model_load_window": true,
  "desktop.preset_save_confirm": false,
};

/** A behaviour in force and where its value comes from. */
export interface GeneralBehaviour {
  value: boolean;
  source: SettingSource;
}

const ALL_KEYS = Object.values(GENERAL_BEHAVIOUR_KEYS) as GeneralBehaviourKey[];

/** Every behaviour at its default, as before the shell answers. */
export function generalBehaviourDefaults(): Record<GeneralBehaviourKey, GeneralBehaviour> {
  const entries = ALL_KEYS.map((key) => [
    key,
    { value: GENERAL_BEHAVIOUR_DEFAULTS[key], source: "default" },
  ]);
  return Object.fromEntries(entries) as Record<GeneralBehaviourKey, GeneralBehaviour>;
}

/** The behaviours in force, the defaults until the shell answers. */
export const generalBehaviours = writable<Record<GeneralBehaviourKey, GeneralBehaviour>>(
  generalBehaviourDefaults(),
);

/** The two shell calls the store makes, replaceable in tests. */
export interface GeneralBehavioursIpc {
  read: (key: string) => Promise<ResolvedSetting>;
  write: (key: string, value: unknown) => Promise<ResolvedSetting>;
}

const shellIpc: GeneralBehavioursIpc = { read: getResolvedSetting, write: setSetting };

function apply(key: GeneralBehaviourKey, resolved: unknown): void {
  if (typeof resolved !== "object" || resolved === null) return;
  const { value, source } = resolved as Partial<ResolvedSetting>;
  generalBehaviours.update((all) => ({
    ...all,
    [key]: {
      value: typeof value === "boolean" ? value : GENERAL_BEHAVIOUR_DEFAULTS[key],
      source: source ?? all[key].source,
    },
  }));
}

/** Seeds every behaviour from the value in force. A failure keeps its default. */
export async function hydrateGeneralBehaviours(
  ipc: GeneralBehavioursIpc = shellIpc,
): Promise<void> {
  await Promise.all(
    ALL_KEYS.map(async (key) => {
      try {
        apply(key, await ipc.read(key));
      } catch {
        // Shell unreachable: this behaviour stays at its default.
      }
    }),
  );
}

/**
 * Writes one behaviour. The switch moves at once and comes back when the
 * shell refuses the write (a value the policy imposes, an unreadable file).
 * Resolves to whether the write went through.
 */
export async function setGeneralBehaviour(
  key: GeneralBehaviourKey,
  value: boolean,
  ipc: GeneralBehavioursIpc = shellIpc,
): Promise<boolean> {
  const previous = get(generalBehaviours)[key];
  if (previous.source === "policy") return false;
  generalBehaviours.update((all) => ({ ...all, [key]: { ...all[key], value } }));
  try {
    apply(key, await ipc.write(key, value));
    return true;
  } catch {
    generalBehaviours.update((all) => ({ ...all, [key]: previous }));
    return false;
  }
}

function valueOf(key: GeneralBehaviourKey): Readable<boolean> {
  return derived(generalBehaviours, ($all) => $all[key].value);
}

/** Whether choosing a model that is not in memory opens the loading window. */
export const modelLoadWindow = valueOf(GENERAL_BEHAVIOUR_KEYS.modelLoadWindow);

/** Whether saving into a preset asks for a confirmation first. */
export const presetSaveConfirm = valueOf(GENERAL_BEHAVIOUR_KEYS.presetSaveConfirm);
