/**
 * Detail sidebar state: the collapsible column on the right of the screen.
 *
 * It carries the contextual tabs of the current screen. The Guide tab is on
 * every screen and always comes last; the help icon of the top bar, the
 * Cmd+/ (Ctrl+/ elsewhere) chord and the palette open the column on it.
 */
import { derived, get, writable, type Readable } from "svelte/store";

/** Tabs the column can show. The Guide tab exists on every screen. */
export type DetailSidebarTab = "guide";

export interface DetailSidebarState {
  /** `true` while the column is shown. */
  open: boolean;
  /** The tab shown when the column is open. */
  tab: DetailSidebarTab;
}

/** Width of the open column, in rem. */
export const DETAIL_SIDEBAR_WIDTH_REM = 22;

const INITIAL: DetailSidebarState = { open: false, tab: "guide" };

const state = writable<DetailSidebarState>({ ...INITIAL });

/** `true` while the column is open on the Guide tab. */
export const guideVisible: Readable<boolean> = derived(
  state,
  ($s) => $s.open && $s.tab === "guide",
);

export const detailSidebar = {
  subscribe: state.subscribe,

  /** Collapses the column, keeping the last tab for the next opening. */
  close(): void {
    state.update((s) => ({ ...s, open: false }));
  },

  /** Opens or collapses the column on its last tab (top bar icon). */
  toggle(): void {
    state.update((s) => ({ ...s, open: !s.open }));
  },

  /** Opens the column on the Guide tab. */
  openGuide(): void {
    state.set({ open: true, tab: "guide" });
  },

  /**
   * Opens the column on the Guide tab, or collapses it when the Guide tab is
   * already the one shown (help icon, Cmd+/ and the palette entry).
   */
  toggleGuide(): void {
    if (get(guideVisible)) {
      state.update((s) => ({ ...s, open: false }));
      return;
    }
    state.set({ open: true, tab: "guide" });
  },

  /** Restores the initial state (tests). */
  reset(): void {
    state.set({ ...INITIAL });
  },
};
