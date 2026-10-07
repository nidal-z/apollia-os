/**
 * Whether each conversation of the list shows the tokens generated for it.
 *
 * Off by default. A display preference: it changes what the list shows and
 * nothing the application does, so it is persisted the way the theme is
 * (`theme.ts`) and never imposed by the organization policy.
 */
import { writable } from "svelte/store";

export const CHAT_TOKEN_COUNT_DEFAULT = false;

export const CHAT_TOKEN_COUNT_STORAGE_KEY = "apollia-chat-token-count";

/** A stored value, or the default when it is missing or unknown. */
export function parseChatTokenCount(raw: string | null): boolean {
  if (raw === "on") return true;
  if (raw === "off") return false;
  return CHAT_TOKEN_COUNT_DEFAULT;
}

function readStored(): boolean {
  if (typeof localStorage === "undefined") return CHAT_TOKEN_COUNT_DEFAULT;
  try {
    return parseChatTokenCount(localStorage.getItem(CHAT_TOKEN_COUNT_STORAGE_KEY));
  } catch {
    return CHAT_TOKEN_COUNT_DEFAULT;
  }
}

/** Whether the list shows the token count of each conversation. */
export const chatTokenCount = writable<boolean>(readStored());

chatTokenCount.subscribe((value) => {
  if (typeof localStorage === "undefined") return;
  try {
    localStorage.setItem(CHAT_TOKEN_COUNT_STORAGE_KEY, value ? "on" : "off");
  } catch {
    /* quota or privacy mode: the choice lives for this session only */
  }
});

/**
 * A token count as the list shows it: the exact figure under a thousand, then
 * thousands and millions with one decimal, in the reader's language.
 */
export function formatTokenCount(tokens: number, locale: string): string {
  const safe = Number.isFinite(tokens) && tokens > 0 ? Math.round(tokens) : 0;
  return new Intl.NumberFormat(locale, {
    notation: "compact",
    maximumFractionDigits: 1,
  }).format(safe);
}
