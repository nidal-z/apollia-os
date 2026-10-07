// How the chat behaves, as Settings > Chat sets it: one empty conversation at
// a time, unloading the loaded models before another is loaded, the two
// double clicks, and the regeneration shortcut.
//
// Each is one key of `apollia.toml`, read and written through the shell, so
// the organization policy can impose any of them. The engines read the
// unloading rule themselves, at every load; the four others decide what the
// interface does, so the store is seeded at launch.

import { derived, get, writable, type Readable } from "svelte/store";
import {
  getResolvedSetting,
  setSetting,
  type ResolvedSetting,
  type SettingSource,
} from "$lib/ipc/settings";

/** The dotted keys, one per behaviour, never repeated elsewhere. */
export const CHAT_BEHAVIOUR_KEYS = {
  singleEmpty: "desktop.chat_single_empty",
  unloadBeforeLoad: "llm.residency.unload_before_load",
  doubleClickEdit: "desktop.chat_double_click_edit",
  doubleClickRename: "desktop.chat_double_click_rename",
  regenerateShortcut: "desktop.chat_regenerate_shortcut",
} as const;

/** One of the keys above. */
export type ChatBehaviourKey = (typeof CHAT_BEHAVIOUR_KEYS)[keyof typeof CHAT_BEHAVIOUR_KEYS];

/** The value of each behaviour when no one wrote one, as Rust defaults it. */
export const CHAT_BEHAVIOUR_DEFAULTS: Record<ChatBehaviourKey, boolean> = {
  "desktop.chat_single_empty": true,
  "llm.residency.unload_before_load": false,
  "desktop.chat_double_click_edit": false,
  "desktop.chat_double_click_rename": true,
  "desktop.chat_regenerate_shortcut": true,
};

/** A behaviour in force and where its value comes from. */
export interface ChatBehaviour {
  value: boolean;
  source: SettingSource;
}

const ALL_KEYS = Object.values(CHAT_BEHAVIOUR_KEYS) as ChatBehaviourKey[];

function defaults(): Record<ChatBehaviourKey, ChatBehaviour> {
  const entries = ALL_KEYS.map((key) => [
    key,
    { value: CHAT_BEHAVIOUR_DEFAULTS[key], source: "default" },
  ]);
  return Object.fromEntries(entries) as Record<ChatBehaviourKey, ChatBehaviour>;
}

/** The behaviours in force, the defaults until the shell answers. */
export const chatBehaviours = writable<Record<ChatBehaviourKey, ChatBehaviour>>(defaults());

/** The two shell calls the store makes, replaceable in tests. */
export interface ChatBehavioursIpc {
  read: (key: string) => Promise<ResolvedSetting>;
  write: (key: string, value: unknown) => Promise<ResolvedSetting>;
}

const shellIpc: ChatBehavioursIpc = { read: getResolvedSetting, write: setSetting };

function apply(key: ChatBehaviourKey, resolved: unknown): void {
  if (typeof resolved !== "object" || resolved === null) return;
  const { value, source } = resolved as Partial<ResolvedSetting>;
  chatBehaviours.update((all) => ({
    ...all,
    [key]: {
      value: typeof value === "boolean" ? value : CHAT_BEHAVIOUR_DEFAULTS[key],
      source: source ?? all[key].source,
    },
  }));
}

/** Seeds every behaviour from the value in force. A failure keeps its default. */
export async function hydrateChatBehaviours(ipc: ChatBehavioursIpc = shellIpc): Promise<void> {
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
export async function setChatBehaviour(
  key: ChatBehaviourKey,
  value: boolean,
  ipc: ChatBehavioursIpc = shellIpc,
): Promise<boolean> {
  const previous = get(chatBehaviours)[key];
  if (previous.source === "policy") return false;
  chatBehaviours.update((all) => ({ ...all, [key]: { ...all[key], value } }));
  try {
    apply(key, await ipc.write(key, value));
    return true;
  } catch {
    chatBehaviours.update((all) => ({ ...all, [key]: previous }));
    return false;
  }
}

function valueOf(key: ChatBehaviourKey): Readable<boolean> {
  return derived(chatBehaviours, ($all) => $all[key].value);
}

/** Whether "New conversation" opens the empty conversation that exists. */
export const chatSingleEmpty = valueOf(CHAT_BEHAVIOUR_KEYS.singleEmpty);

/** Whether a double click on a message opens its edition. */
export const chatDoubleClickEdit = valueOf(CHAT_BEHAVIOUR_KEYS.doubleClickEdit);

/** Whether a double click on a conversation or a folder renames it. */
export const chatDoubleClickRename = valueOf(CHAT_BEHAVIOUR_KEYS.doubleClickRename);

/** Whether the regeneration shortcut regenerates the last answer. */
export const chatRegenerateShortcut = valueOf(CHAT_BEHAVIOUR_KEYS.regenerateShortcut);
