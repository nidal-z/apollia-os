/**
 * Reading comfort of the chat: text size and weight, column width, density,
 * and whether reasoning blocks open unfolded.
 *
 * Persisted the way the theme is (`theme.ts`): one localStorage entry read at
 * start-up, written on every change, and mirrored onto `<html>` as data
 * attributes that the type scale of `app.css` reads. No size is computed here;
 * the step only names a rung of `--chat-font-scale`.
 */
import { derived, writable, type Readable } from "svelte/store";

export type ChatFontSize = 1 | 2 | 3 | 4 | 5;
export type ChatFontWeight = "normal" | "medium";
export type ChatDensity = "comfortable" | "compact";

export interface ChatAppearance {
  fontSize: ChatFontSize;
  fontWeight: ChatFontWeight;
  fullWidth: boolean;
  density: ChatDensity;
  expandReasoning: boolean;
}

export const CHAT_FONT_SIZE_MIN: ChatFontSize = 1;
export const CHAT_FONT_SIZE_MAX: ChatFontSize = 5;
export const CHAT_FONT_WEIGHTS: readonly ChatFontWeight[] = ["normal", "medium"];
export const CHAT_DENSITIES: readonly ChatDensity[] = ["comfortable", "compact"];

export const CHAT_APPEARANCE_DEFAULTS: Readonly<ChatAppearance> = Object.freeze({
  fontSize: 3,
  fontWeight: "normal",
  fullWidth: false,
  density: "comfortable",
  expandReasoning: false,
});

export const CHAT_APPEARANCE_STORAGE_KEY = "apollia-chat-appearance";

function isFontSize(v: unknown): v is ChatFontSize {
  return (
    typeof v === "number" &&
    Number.isInteger(v) &&
    v >= CHAT_FONT_SIZE_MIN &&
    v <= CHAT_FONT_SIZE_MAX
  );
}

/**
 * Read a stored value, field by field: a field that is missing or out of range
 * falls back to its default without discarding the others.
 */
export function parseChatAppearance(raw: string | null): ChatAppearance {
  const out: ChatAppearance = { ...CHAT_APPEARANCE_DEFAULTS };
  if (!raw) return out;
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch {
    return out;
  }
  if (typeof parsed !== "object" || parsed === null) return out;
  const p = parsed as Record<string, unknown>;
  if (isFontSize(p.fontSize)) out.fontSize = p.fontSize;
  if ((CHAT_FONT_WEIGHTS as readonly unknown[]).includes(p.fontWeight)) {
    out.fontWeight = p.fontWeight as ChatFontWeight;
  }
  if (typeof p.fullWidth === "boolean") out.fullWidth = p.fullWidth;
  if ((CHAT_DENSITIES as readonly unknown[]).includes(p.density)) {
    out.density = p.density as ChatDensity;
  }
  if (typeof p.expandReasoning === "boolean") out.expandReasoning = p.expandReasoning;
  return out;
}

function readStored(): ChatAppearance {
  if (typeof localStorage === "undefined") return { ...CHAT_APPEARANCE_DEFAULTS };
  try {
    return parseChatAppearance(localStorage.getItem(CHAT_APPEARANCE_STORAGE_KEY));
  } catch {
    return { ...CHAT_APPEARANCE_DEFAULTS };
  }
}

/** The whole preference, one store; the readers below are its projections. */
export const chatAppearance = writable<ChatAppearance>(readStored());

export const chatDensity: Readable<ChatDensity> = derived(chatAppearance, (a) => a.density);
export const chatFullWidth: Readable<boolean> = derived(chatAppearance, (a) => a.fullWidth);
export const reasoningExpandedByDefault: Readable<boolean> = derived(
  chatAppearance,
  (a) => a.expandReasoning,
);

export function setChatAppearance(patch: Partial<ChatAppearance>): void {
  chatAppearance.update((a) => ({ ...a, ...patch }));
}

export function resetChatFontSize(): void {
  setChatAppearance({ fontSize: CHAT_APPEARANCE_DEFAULTS.fontSize });
}

/** Persist the preference and expose it to `app.css` on `<html>`. */
export function applyChatAppearance(a: ChatAppearance): void {
  if (typeof localStorage !== "undefined") {
    try {
      localStorage.setItem(CHAT_APPEARANCE_STORAGE_KEY, JSON.stringify(a));
    } catch {
      /* quota or privacy mode: the choice lives for this session only */
    }
  }
  if (typeof document === "undefined") return;
  const root = document.documentElement;
  root.dataset.chatFontSize = String(a.fontSize);
  root.dataset.chatFontWeight = a.fontWeight;
}

/** Apply the stored preference now and on every change. */
export function initChatAppearance(): void {
  chatAppearance.subscribe(applyChatAppearance);
}
