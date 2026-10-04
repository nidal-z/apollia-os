// Whether the application names a new conversation from its first message:
// never, automatically when the model generates fast enough, or always.
//
// The value lives in `[desktop] chat_auto_naming` of `apollia.toml`, read and
// written through the shell, so the organization policy can impose it.

import { get, writable } from "svelte/store";
import {
  getResolvedSetting,
  setSetting,
  type ResolvedSetting,
  type SettingSource,
} from "$lib/ipc/settings";

/** The dotted key of the setting in `apollia.toml`. */
export const CHAT_AUTO_NAMING_KEY = "desktop.chat_auto_naming";

/** The three choices, in the order Settings > Chat offers them. */
export const CHAT_AUTO_NAMINGS = ["never", "auto", "always"] as const;

export type ChatAutoNaming = (typeof CHAT_AUTO_NAMINGS)[number];

export const CHAT_AUTO_NAMING_DEFAULT: ChatAutoNaming = "auto";

/** The choice in force. Automatic until the shell answers. */
export const chatAutoNaming = writable<ChatAutoNaming>(CHAT_AUTO_NAMING_DEFAULT);

/** Where the value comes from; `"policy"` locks the choice. */
export const chatAutoNamingSource = writable<SettingSource>("default");

/** The two shell calls the store makes, replaceable in tests. */
export interface ChatAutoNamingIpc {
  read: (key: string) => Promise<ResolvedSetting>;
  write: (key: string, value: unknown) => Promise<ResolvedSetting>;
}

const shellIpc: ChatAutoNamingIpc = { read: getResolvedSetting, write: setSetting };

export function isChatAutoNaming(value: unknown): value is ChatAutoNaming {
  return (CHAT_AUTO_NAMINGS as readonly unknown[]).includes(value);
}

function apply(resolved: unknown): void {
  if (typeof resolved !== "object" || resolved === null) return;
  const { value, source } = resolved as Partial<ResolvedSetting>;
  chatAutoNaming.set(isChatAutoNaming(value) ? value : CHAT_AUTO_NAMING_DEFAULT);
  if (source) chatAutoNamingSource.set(source);
}

/** Seeds the store from the value in force. A failure keeps the default. */
export async function hydrateChatAutoNaming(
  ipc: ChatAutoNamingIpc = shellIpc,
): Promise<void> {
  try {
    apply(await ipc.read(CHAT_AUTO_NAMING_KEY));
  } catch {
    // Shell unreachable: automatic naming stays.
  }
}

/**
 * Chooses the naming. The radio moves at once and comes back when the shell
 * refuses the write (a value the policy imposes, an unreadable file).
 * Resolves to whether the write went through.
 */
export async function setChatAutoNaming(
  choice: ChatAutoNaming,
  ipc: ChatAutoNamingIpc = shellIpc,
): Promise<boolean> {
  const previous = get(chatAutoNaming);
  chatAutoNaming.set(choice);
  try {
    apply(await ipc.write(CHAT_AUTO_NAMING_KEY, choice));
    return true;
  } catch {
    chatAutoNaming.set(previous);
    return false;
  }
}
