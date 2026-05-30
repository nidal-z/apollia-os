/**
 * Chat layout store.
 *
 * Drives the 3-column chat shell (Sessions / Conversation / ContextDrawer).
 *
 * Responsive contract — one source of truth for the shell:
 *   - `sm`  (<768 px)   → sessions = drawer overlay, context = drawer overlay
 *   - `md`  (768–1023)  → sessions = sticky column, context = drawer overlay
 *   - `lg+` (≥1024 px)  → sessions = column, context = inline column (toggle)
 *
 * The ContextDrawer width (`lg+` only) is persisted in `localStorage` under
 * `chat.contextDrawer.width`, clamped to the canonical bounds in tokens.ts.
 */
import { derived, readable, writable, type Readable } from "svelte/store";
import { chatLayout } from "$lib/design/tokens";

export type ContextDrawerMode = "overlay" | "inline";
export type SessionsPaneMode = "drawer" | "sticky" | "column";
type Viewport = "sm" | "md" | "lg";

const WIDTH_STORAGE_KEY = "chat.contextDrawer.width";
const MD_QUERY = "(min-width: 768px)";
const LG_QUERY = "(min-width: 1024px)";

function computeViewport(): Viewport {
  if (globalThis.window === undefined) return "lg";
  if (globalThis.matchMedia(LG_QUERY).matches) return "lg";
  if (globalThis.matchMedia(MD_QUERY).matches) return "md";
  return "sm";
}

function clampWidth(px: number): number {
  return Math.min(chatLayout.contextMaxPx, Math.max(chatLayout.contextMinPx, Math.round(px)));
}

function loadWidth(): number {
  if (typeof localStorage === "undefined") return chatLayout.contextDefaultPx;
  try {
    const raw = localStorage.getItem(WIDTH_STORAGE_KEY);
    if (!raw) return chatLayout.contextDefaultPx;
    const n = Number(raw);
    if (!Number.isFinite(n)) return chatLayout.contextDefaultPx;
    return clampWidth(n);
  } catch {
    return chatLayout.contextDefaultPx;
  }
}

export const chatViewport: Readable<Viewport> = readable<Viewport>(computeViewport(), (set) => {
  if (globalThis.window === undefined) return;
  const mdMql = globalThis.matchMedia(MD_QUERY);
  const lgMql = globalThis.matchMedia(LG_QUERY);
  const update = () => set(computeViewport());
  mdMql.addEventListener("change", update);
  lgMql.addEventListener("change", update);
  return () => {
    mdMql.removeEventListener("change", update);
    lgMql.removeEventListener("change", update);
  };
});

export const contextDrawerOpen = writable<boolean>(false);
export const sessionsDrawerOpen = writable<boolean>(false);
/**
 * Sessions sidebar collapsed state (`lg+` only — `Cmd+B` toggle from
 *). On smaller viewports the sidebar is a drawer governed by
 * `sessionsDrawerOpen`; the collapsed flag is ignored.
 */
export const sessionsSidebarCollapsed = writable<boolean>(false);

export function toggleSessionsSidebar(): void {
  sessionsSidebarCollapsed.update((c) => !c);
}

/** Active tab in the ContextDrawer. Writable so global shortcuts can jump to a specific tab. */
export type ContextDrawerTab = "config" | "metrics" | "memory" | "artifacts";
export const contextDrawerActiveTab = writable<ContextDrawerTab>("config");

export const contextDrawerWidth = writable<number>(loadWidth());
contextDrawerWidth.subscribe((w) => {
  if (typeof localStorage === "undefined") return;
  try {
    localStorage.setItem(WIDTH_STORAGE_KEY, String(clampWidth(w)));
  } catch {
    // Quota or disabled storage — silently ignore.
  }
});

export const contextDrawerMode: Readable<ContextDrawerMode> = derived(
  chatViewport,
  ($v) => ($v === "lg" ? "inline" : "overlay"),
);

function deriveSessionsPaneMode(v: Viewport): SessionsPaneMode {
  if (v === "sm") return "drawer";
  if (v === "md") return "sticky";
  return "column";
}

export const sessionsPaneMode: Readable<SessionsPaneMode> = derived(
  chatViewport,
  ($v) => deriveSessionsPaneMode($v),
);

export function setContextDrawerWidth(px: number): void {
  contextDrawerWidth.set(clampWidth(px));
}

export function toggleContextDrawer(): void {
  contextDrawerOpen.update((o) => !o);
}

export function openSessionsDrawer(): void { sessionsDrawerOpen.set(true); }
export function closeSessionsDrawer(): void { sessionsDrawerOpen.set(false); }
