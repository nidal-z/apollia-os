// The channel the engines follow, and the check of what it offers.
//
// Both go through the engine store of the runtime: the channel is written by
// its settings route and read back with the engine list, never kept here.

import {
  checkEngineUpdates,
  setEngineChannel,
  type EngineChannel,
} from "$lib/ipc/system";
import { settingsLoaders } from "$lib/stores/settings";

/** The channels, in the order the list shows them. */
export const ENGINE_CHANNELS: readonly EngineChannel[] = ["stable", "preview"];

/** The i18n key of each channel's name. */
export const ENGINE_CHANNEL_LABEL_KEYS: Record<EngineChannel, string> = {
  stable: "settings.engines.channel_stable",
  preview: "settings.engines.channel_preview",
};

export function isEngineChannel(value: string): value is EngineChannel {
  return (ENGINE_CHANNELS as readonly string[]).includes(value);
}

function message(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

/**
 * Check the followed channel, then read the engine list again so the update
 * of each engine is the one just verified. Returns the refusal of the check,
 * or `null`; the list is read again either way.
 */
export async function verifyEngineUpdates(): Promise<string | null> {
  let refusal: string | null = null;
  try {
    await checkEngineUpdates();
  } catch (err) {
    refusal = message(err);
  }
  await settingsLoaders.engines(true);
  return refusal;
}

/**
 * Follow `channel`: write it through the store, then check it and read the
 * engine list again. Returns what was refused, or `null`. A channel that was
 * not written checks nothing.
 */
export async function changeEngineChannel(channel: EngineChannel): Promise<string | null> {
  try {
    await setEngineChannel(channel);
  } catch (err) {
    return message(err);
  }
  return verifyEngineUpdates();
}
