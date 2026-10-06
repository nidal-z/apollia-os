// Whether a new conversation is born with the profile shared with the
// built-in assistant.
//
// The value lives in `[chat] share_profile_default` of `apollia.toml`, set in
// Settings > Integrations. The runtime gives it to every conversation it
// creates; this store shows the same position before the conversation exists.

import { writable } from "svelte/store";
import { getResolvedSetting, type ResolvedSetting } from "$lib/ipc/settings";

/** The dotted key of the setting in `apollia.toml`. */
export const SHARE_PROFILE_DEFAULT_KEY = "chat.share_profile_default";

/** The default in force. Shared until the shell answers. */
export const shareProfileDefault = writable<boolean>(true);

/** Seeds the store from the value in force. A failure keeps the profile shared. */
export async function hydrateShareProfileDefault(
  read: (key: string) => Promise<ResolvedSetting> = getResolvedSetting,
): Promise<void> {
  try {
    const resolved = await read(SHARE_PROFILE_DEFAULT_KEY);
    shareProfileDefault.set(resolved.value !== false);
  } catch {
    // Shell unreachable: the default of a fresh installation stays.
  }
}
