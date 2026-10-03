// The update channel: stable by default, preview to receive the trial versions
// as well.
//
// The value lives in `[desktop] update_channel` of `apollia.toml`, read and
// written through the shell, so the organization policy can impose it and the
// updater, on the Rust side, asks the manifest of the channel in force.

import { get, writable } from "svelte/store";
import {
  getResolvedSetting,
  setSetting,
  type ResolvedSetting,
  type SettingSource,
} from "$lib/ipc/settings";

/** The dotted key of the setting in `apollia.toml`. */
export const UPDATE_CHANNEL_KEY = "desktop.update_channel";

/** The two channels, in the order the selector offers them. */
export const UPDATE_CHANNELS = ["stable", "preview"] as const;

/** A channel the updater can ask. */
export type UpdateChannel = (typeof UPDATE_CHANNELS)[number];

/** The channel in force. Stable until the shell answers. */
export const updateChannel = writable<UpdateChannel>("stable");

/** Where the value comes from; `"policy"` locks the selector. */
export const updateChannelSource = writable<SettingSource>("default");

/** The two shell calls the store makes, replaceable in tests. */
export interface UpdateChannelIpc {
  read: (key: string) => Promise<ResolvedSetting>;
  write: (key: string, value: unknown) => Promise<ResolvedSetting>;
}

const shellIpc: UpdateChannelIpc = { read: getResolvedSetting, write: setSetting };

/** Whether `value` names a channel; anything else reads as stable. */
export function isUpdateChannel(value: unknown): value is UpdateChannel {
  return (UPDATE_CHANNELS as readonly unknown[]).includes(value);
}

/** The state of the selector for a source: a value the policy imposes locks it. */
export function updateChannelSelectState(source: SettingSource): {
  disabled: boolean;
  locked: boolean;
} {
  const locked = source === "policy";
  return { disabled: locked, locked };
}

function apply(resolved: unknown): void {
  if (typeof resolved !== "object" || resolved === null) return;
  const { value, source } = resolved as Partial<ResolvedSetting>;
  updateChannel.set(isUpdateChannel(value) ? value : "stable");
  if (source) updateChannelSource.set(source);
}

/** Seeds the store from the value in force. A failure keeps stable. */
export async function hydrateUpdateChannel(ipc: UpdateChannelIpc = shellIpc): Promise<void> {
  try {
    apply(await ipc.read(UPDATE_CHANNEL_KEY));
  } catch {
    // Shell unreachable: the channel stays stable.
  }
}

/**
 * Chooses the update channel. The selector moves at once and comes back when
 * the shell refuses the write (a value the policy imposes, an unreadable
 * file). Resolves to whether the write went through.
 */
export async function setUpdateChannel(
  channel: UpdateChannel,
  ipc: UpdateChannelIpc = shellIpc,
): Promise<boolean> {
  const previous = get(updateChannel);
  updateChannel.set(channel);
  try {
    apply(await ipc.write(UPDATE_CHANNEL_KEY, channel));
    return true;
  } catch {
    updateChannel.set(previous);
    return false;
  }
}
