/**
 * The key that sends a message from the chat composer.
 *
 * Enter sends and Shift + Enter breaks the line, by default. The other choice
 * swaps them: Cmd + Enter (Ctrl + Enter outside the Mac) sends and Enter
 * breaks the line. A display preference, persisted the way the theme is
 * (`theme.ts`) and never imposed by the organization policy.
 */
import { writable } from "svelte/store";

export type ChatSendKey = "enter" | "mod-enter";

export const CHAT_SEND_KEYS: readonly ChatSendKey[] = ["enter", "mod-enter"];

export const CHAT_SEND_KEY_DEFAULT: ChatSendKey = "enter";

export const CHAT_SEND_KEY_STORAGE_KEY = "apollia-chat-send-key";

/** A stored value, or the default when it is missing or unknown. */
export function parseChatSendKey(raw: string | null): ChatSendKey {
  return (CHAT_SEND_KEYS as readonly (string | null)[]).includes(raw)
    ? (raw as ChatSendKey)
    : CHAT_SEND_KEY_DEFAULT;
}

function readStored(): ChatSendKey {
  if (typeof localStorage === "undefined") return CHAT_SEND_KEY_DEFAULT;
  try {
    return parseChatSendKey(localStorage.getItem(CHAT_SEND_KEY_STORAGE_KEY));
  } catch {
    return CHAT_SEND_KEY_DEFAULT;
  }
}

export const chatSendKey = writable<ChatSendKey>(readStored());

chatSendKey.subscribe((value) => {
  if (typeof localStorage === "undefined") return;
  try {
    localStorage.setItem(CHAT_SEND_KEY_STORAGE_KEY, value);
  } catch {
    /* quota or privacy mode: the choice lives for this session only */
  }
});

/** The part of a key event the decision reads. */
export interface ComposerKey {
  key: string;
  shiftKey: boolean;
  metaKey: boolean;
  ctrlKey: boolean;
  isComposing?: boolean;
}

/**
 * What a key pressed in the composer does under `mode`: `"send"` sends the
 * message, `"newline"` lets the textarea break the line, `null` is not a
 * key of the choice. A key typed while an input method composes a character
 * never sends.
 */
export function composerEnterAction(
  event: ComposerKey,
  mode: ChatSendKey,
): "send" | "newline" | null {
  if (event.key !== "Enter") return null;
  if (event.isComposing) return "newline";
  const mod = event.metaKey || event.ctrlKey;
  if (mode === "mod-enter") return mod ? "send" : "newline";
  return event.shiftKey ? "newline" : "send";
}
