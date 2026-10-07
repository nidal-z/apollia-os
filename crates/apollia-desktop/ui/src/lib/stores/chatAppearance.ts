/**
 * The display preferences of Settings > Appearance: the side of the navigation
 * bar, and how the chat reads (rendered or plain text, text size and weight,
 * column width, density, bubbles or blocks, which answers show their generation
 * figures, how the thread scrolls on send and while an answer arrives, how
 * reasoning blocks open and fade, and whether the split chat draws its scroll
 * bars).
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
export type NavPosition = "left" | "right";
export type ChatViewMode = "markdown" | "plain";
export type GenerationInfoScope = "last" | "all";
export type ChatMessageStyle = "bubble" | "block";

export interface ChatAppearance {
  /** Side of the window the navigation bar sits on. */
  navPosition: NavPosition;
  /** An answer rendered as Markdown, or shown as the text received. */
  viewMode: ChatViewMode;
  /** The tabs and the pane area of the split chat draw a scroll bar on overflow. */
  tabScrollbar: boolean;
  /** Generation figures under the last answer only, or under every answer. */
  generationInfo: GenerationInfoScope;
  /** On send, the sent message scrolls to the top of the thread. */
  scrollSentToTop: boolean;
  /** The thread follows the answer while it arrives. */
  followGeneration: boolean;
  /** A user message in a bubble, or as a block the width of the column. */
  messageStyle: ChatMessageStyle;
  /** A reasoning block still arriving fades out at its clipped edge. */
  reasoningVignette: boolean;
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
export const NAV_POSITIONS: readonly NavPosition[] = ["left", "right"];
export const CHAT_VIEW_MODES: readonly ChatViewMode[] = ["markdown", "plain"];
export const GENERATION_INFO_SCOPES: readonly GenerationInfoScope[] = ["last", "all"];
export const CHAT_MESSAGE_STYLES: readonly ChatMessageStyle[] = ["bubble", "block"];

export const CHAT_APPEARANCE_DEFAULTS: Readonly<ChatAppearance> = Object.freeze({
  navPosition: "left",
  viewMode: "markdown",
  tabScrollbar: false,
  generationInfo: "last",
  scrollSentToTop: true,
  followGeneration: true,
  messageStyle: "bubble",
  reasoningVignette: true,
  fontSize: 3,
  fontWeight: "normal",
  fullWidth: false,
  density: "comfortable",
  expandReasoning: false,
});

export const CHAT_APPEARANCE_STORAGE_KEY = "apollia-chat-appearance";

function oneOf<T extends string>(allowed: readonly T[], v: unknown): v is T {
  return (allowed as readonly unknown[]).includes(v);
}

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
  if (oneOf(NAV_POSITIONS, p.navPosition)) out.navPosition = p.navPosition;
  if (oneOf(CHAT_VIEW_MODES, p.viewMode)) out.viewMode = p.viewMode;
  if (typeof p.tabScrollbar === "boolean") out.tabScrollbar = p.tabScrollbar;
  if (oneOf(GENERATION_INFO_SCOPES, p.generationInfo)) out.generationInfo = p.generationInfo;
  if (typeof p.scrollSentToTop === "boolean") out.scrollSentToTop = p.scrollSentToTop;
  if (typeof p.followGeneration === "boolean") out.followGeneration = p.followGeneration;
  if (oneOf(CHAT_MESSAGE_STYLES, p.messageStyle)) out.messageStyle = p.messageStyle;
  if (typeof p.reasoningVignette === "boolean") out.reasoningVignette = p.reasoningVignette;
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
export const navPosition: Readable<NavPosition> = derived(chatAppearance, (a) => a.navPosition);
export const chatViewMode: Readable<ChatViewMode> = derived(chatAppearance, (a) => a.viewMode);
export const paneScrollbar: Readable<boolean> = derived(chatAppearance, (a) => a.tabScrollbar);
export const generationInfoScope: Readable<GenerationInfoScope> = derived(
  chatAppearance,
  (a) => a.generationInfo,
);
export const chatMessageStyle: Readable<ChatMessageStyle> = derived(
  chatAppearance,
  (a) => a.messageStyle,
);
export const reasoningVignette: Readable<boolean> = derived(
  chatAppearance,
  (a) => a.reasoningVignette,
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
