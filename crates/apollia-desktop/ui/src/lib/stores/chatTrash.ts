/**
 * The trash of the conversations.
 *
 * Removing a conversation sends it to the trash of Apollia, from where it is
 * restored with its messages. The choice is on by default; off, removing a
 * conversation erases it for good, as it always did. The choice decides which
 * command the interface calls, nothing on the Rust side reads it, so it is
 * persisted the way the send key is (`chatSendKey.ts`).
 */
import { get, writable } from "svelte/store";
import {
  deleteChatSession,
  listTrashedChatSessions,
  restoreChatSession,
  trashChatSession,
  type TrashedChatSession,
} from "$lib/ipc/chat";

export const CHAT_TRASH_DEFAULT = true;

export const CHAT_TRASH_STORAGE_KEY = "apollia-chat-trash";

/** A stored value, or the default when it is missing or unknown. */
export function parseChatTrash(raw: string | null): boolean {
  if (raw === "on") return true;
  if (raw === "off") return false;
  return CHAT_TRASH_DEFAULT;
}

function readStored(): boolean {
  if (typeof localStorage === "undefined") return CHAT_TRASH_DEFAULT;
  try {
    return parseChatTrash(localStorage.getItem(CHAT_TRASH_STORAGE_KEY));
  } catch {
    return CHAT_TRASH_DEFAULT;
  }
}

/** Whether a removed conversation goes to the trash. */
export const chatTrashEnabled = writable<boolean>(readStored());

chatTrashEnabled.subscribe((value) => {
  if (typeof localStorage === "undefined") return;
  try {
    localStorage.setItem(CHAT_TRASH_STORAGE_KEY, value ? "on" : "off");
  } catch {
    /* quota or privacy mode: the choice lives for this session only */
  }
});

/**
 * Remove a conversation from the list the way the choice says: to the trash
 * when it is on, for good when it is off.
 */
export function removeConversation(
  sessionId: string,
  toTrash: boolean = get(chatTrashEnabled),
): Promise<void> {
  return toTrash ? trashChatSession(sessionId) : deleteChatSession(sessionId);
}

/** The i18n key of what the confirmation of a removal says. */
export function removalConfirmKey(toTrash: boolean): string {
  return toTrash ? "chat.trash.confirm_to_trash" : "chat.delete_confirm_message";
}

/** The conversations in the trash, the last removed first. */
export const trashedSessions = writable<TrashedChatSession[]>([]);

/** Read the trash from the runtime. A failed read leaves it as it was. */
export async function loadTrash(): Promise<boolean> {
  try {
    trashedSessions.set(await listTrashedChatSessions());
    return true;
  } catch (err: unknown) {
    console.warn("list_trashed_chat_sessions failed:", err);
    return false;
  }
}

function dropFromTrash(sessionId: string): void {
  trashedSessions.update((sessions) => sessions.filter((s) => s.id !== sessionId));
}

/**
 * Take a conversation out of the trash. The runtime puts it back in the
 * list; the caller reads the list again.
 */
export async function restoreFromTrash(sessionId: string): Promise<void> {
  await restoreChatSession(sessionId);
  dropFromTrash(sessionId);
}

/** Erase a conversation of the trash for good. */
export async function eraseFromTrash(sessionId: string): Promise<void> {
  await deleteChatSession(sessionId);
  dropFromTrash(sessionId);
}

/** A text of the catalogue and its values. */
export interface TrashedWhen {
  key: string;
  values?: Record<string, string>;
}

const DAY_MS = 86_400_000;

function startOfDay(date: Date): number {
  return new Date(date.getFullYear(), date.getMonth(), date.getDate()).getTime();
}

/**
 * When a conversation was removed, in words: today, yesterday, or its date in
 * the reader's language. A date that does not parse reads as no date.
 */
export function trashedWhen(deletedAt: string, now: Date, locale: string): TrashedWhen {
  const removed = new Date(deletedAt);
  if (Number.isNaN(removed.getTime())) return { key: "chat.trash.removed" };
  const days = Math.round((startOfDay(now) - startOfDay(removed)) / DAY_MS);
  if (days <= 0) return { key: "chat.trash.removed_today" };
  if (days === 1) return { key: "chat.trash.removed_yesterday" };
  const date = removed.toLocaleDateString(locale, { day: "numeric", month: "short" });
  return { key: "chat.trash.removed_on", values: { date } };
}
