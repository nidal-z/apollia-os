/**
 * The settings each conversation holds for itself, as the interface last read
 * or wrote them.
 *
 * Several surfaces show the same setting of one conversation, so they read it
 * from one place and a write by one is seen by the others. A conversation
 * whose settings were not read yet, or could not be read, has no entry.
 */
import { derived, writable, type Readable } from "svelte/store";
import { getChatSessionSettings, setChatSessionSettings } from "$lib/ipc/chat";
import type { SessionSettings } from "$lib/types";

const settingsBySession = writable<Record<string, SessionSettings>>({});

function remember(sessionId: string, settings: SessionSettings): void {
  settingsBySession.update((all) => ({ ...all, [sessionId]: settings }));
}

/** The settings of `sessionId`, `null` until they are read. */
export function sessionSettingsStore(sessionId: string): Readable<SessionSettings | null> {
  return derived(settingsBySession, ($all) => $all[sessionId] ?? null);
}

/** Reads the settings of `sessionId` from the runtime and keeps them. */
export async function loadSessionSettings(sessionId: string): Promise<SessionSettings> {
  const settings = await getChatSessionSettings(sessionId);
  remember(sessionId, settings);
  return settings;
}

/**
 * Changes the settings of `sessionId`: reads them again, applies `change`,
 * and replaces them with the result. The runtime replaces the whole record,
 * so the fresh read is what keeps a field another surface wrote meanwhile.
 * A refused write throws and the kept settings are left as they were.
 */
export async function changeSessionSettings(
  sessionId: string,
  change: (current: SessionSettings) => SessionSettings,
): Promise<SessionSettings> {
  const current = await getChatSessionSettings(sessionId);
  const stored = await setChatSessionSettings(sessionId, change(current));
  remember(sessionId, stored);
  return stored;
}
