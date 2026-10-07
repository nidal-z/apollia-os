/**
 * Whether the right panel of the chat always shows the prompt template the
 * model of the conversation receives.
 *
 * Off by default, and read in developer mode only. A display preference: it
 * changes what the panel shows and nothing the application does, so it is
 * persisted the way the theme is (`theme.ts`) and never imposed by the
 * organization policy.
 */
import { writable } from "svelte/store";

export const CHAT_PROMPT_TEMPLATE_DEFAULT = false;

export const CHAT_PROMPT_TEMPLATE_STORAGE_KEY = "apollia-chat-prompt-template";

/** A stored value, or the default when it is missing or unknown. */
export function parseChatPromptTemplate(raw: string | null): boolean {
  if (raw === "on") return true;
  if (raw === "off") return false;
  return CHAT_PROMPT_TEMPLATE_DEFAULT;
}

function readStored(): boolean {
  if (typeof localStorage === "undefined") return CHAT_PROMPT_TEMPLATE_DEFAULT;
  try {
    return parseChatPromptTemplate(localStorage.getItem(CHAT_PROMPT_TEMPLATE_STORAGE_KEY));
  } catch {
    return CHAT_PROMPT_TEMPLATE_DEFAULT;
  }
}

/** Whether the panel of the chat shows the prompt template. */
export const chatPromptTemplate = writable<boolean>(readStored());

chatPromptTemplate.subscribe((value) => {
  if (typeof localStorage === "undefined") return;
  try {
    localStorage.setItem(CHAT_PROMPT_TEMPLATE_STORAGE_KEY, value ? "on" : "off");
  } catch {
    /* quota or privacy mode: the choice lives for this session only */
  }
});
