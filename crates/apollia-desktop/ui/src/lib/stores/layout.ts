/**
 * Layout store - the source of truth for the sidebar and companion state.
 *
 * Derives `sidebarState` from the viewport through `window.matchMedia`:
 *   - >= `lg` (1024 px) -> `expanded` or `icon`, per the user preference
 * - `md`-`lg` -> `icon` forced (auto-collapse)
 *   - < `md`          → `drawer` overlay
 *
 * The `expanded` / `collapsed` preference at `lg+` is persisted in
 * `localStorage` under the key `apollia.ui.sidebar`.
 */
import { derived, get, readable, writable, type Readable } from "svelte/store";
import { companionStore } from "./companion";

/** Effective state of the sidebar at a given moment. */
export type SidebarState = "expanded" | "icon" | "drawer";

/** Persisted user preference. It only applies at `lg+`. */
type SidebarPreference = "expanded" | "collapsed";

type Viewport = "sm" | "md" | "lg";

// Legacy single-key storage - migrated on first load.
const LEGACY_KEY = "apollia.ui.sidebar";
// Per-breakpoint key prefix. Values : `open | collapsed | hidden`.
const STATE_KEY_PREFIX = "apollia.ui.sidebarState_";
// Canonical breakpoints - see `src/lib/design/breakpoints.md`.
const MD_QUERY = "(min-width: 768px)";
const LG_QUERY = "(min-width: 1024px)";

function computeViewport(): Viewport {
  if (globalThis.window === undefined) return "lg";
  if (globalThis.matchMedia(LG_QUERY).matches) return "lg";
  if (globalThis.matchMedia(MD_QUERY).matches) return "md";
  return "sm";
}

type PersistedState = "open" | "collapsed" | "hidden";

function stateKey(v: Viewport): string {
  return `${STATE_KEY_PREFIX}${v}`;
}

function migrateLegacy(): void {
  if (typeof localStorage === "undefined") return;
  try {
    const legacy = localStorage.getItem(LEGACY_KEY);
    if (!legacy) return;
    const mapped: PersistedState = legacy === "collapsed" ? "collapsed" : "open";
    if (!localStorage.getItem(stateKey("lg"))) {
      localStorage.setItem(stateKey("lg"), mapped);
    }
    localStorage.removeItem(LEGACY_KEY);
  } catch {
    // noop
  }
}

function loadState(v: Viewport): PersistedState {
  if (typeof localStorage === "undefined") {
    return v === "sm" ? "hidden" : "open";
  }
  const raw = localStorage.getItem(stateKey(v));
  if (raw === "open" || raw === "collapsed" || raw === "hidden") return raw;
  return v === "sm" ? "hidden" : "open";
}

function saveState(v: Viewport, state: PersistedState): void {
  if (typeof localStorage === "undefined") return;
  try {
    localStorage.setItem(stateKey(v), state);
  } catch {
    // Quota or disabled storage - silently ignore.
  }
}

function loadPreference(): SidebarPreference {
  migrateLegacy();
  return loadState("lg") === "collapsed" ? "collapsed" : "expanded";
}

function savePreference(pref: SidebarPreference): void {
  saveState("lg", pref === "collapsed" ? "collapsed" : "open");
}

/** Read-only store of the current viewport, updated through `matchMedia`. */
const viewport: Readable<Viewport> = readable<Viewport>(computeViewport(), (set) => {
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

const preference = writable<SidebarPreference>(loadPreference());
preference.subscribe((p) => savePreference(p));

const drawerOpenInternal = writable(false);

// Persist drawer open state under sm - `hidden` when closed, `open` when open.
drawerOpenInternal.subscribe((open) => {
  if (globalThis.window === undefined) return;
  if (computeViewport() !== "sm") return;
  saveState("sm", open ? "open" : "hidden");
});

/**
 * Sidebar state derived from the viewport and the user preference.
 * Mobile first: any viewport under `md` forces `drawer`.
 */
export const sidebarState: Readable<SidebarState> = derived(
  [viewport, preference],
  ([$v, $p]) => {
    if ($v === "sm") return "drawer";
    if ($v === "md") return "icon";
    return $p === "collapsed" ? "icon" : "expanded";
  },
);

/** `true` when the sidebar is rendered as an overlay AND open. */
export const drawerOpen: Readable<boolean> = derived(
  [sidebarState, drawerOpenInternal],
  ([$s, $o]) => $s === "drawer" && $o,
);

/** Mirror of the companion, to observe sidebar and companion together. */
export const companionOpen: Readable<boolean> = derived(
  companionStore,
  ($c) => $c.visible && !$c.minimized,
);

/** Aggregated view - useful for the tests, the telemetry and debugging. */
export const layout: Readable<{
  sidebarState: SidebarState;
  drawerOpen: boolean;
  companionOpen: boolean;
}> = derived(
  [sidebarState, drawerOpen, companionOpen],
  ([$sidebarState, $drawerOpen, $companionOpen]) => ({
    sidebarState: $sidebarState,
    drawerOpen: $drawerOpen,
    companionOpen: $companionOpen,
  }),
);

/**
 * Reactive actions on the layout.
 * - `toggleSidebar`: collapse/expand at `lg+`, open/close the drawer below
 *   `md`, no-op between `md` and `lg` (icon-only is forced).
 */
export const layoutActions = {
  toggleSidebar(): void {
    const v = get(viewport);
    if (v === "sm") {
      drawerOpenInternal.update((o) => !o);
      return;
    }
    if (v === "md") {
      // icon-only forced between md and lg - no user preference here.
      return;
    }
    preference.update((p) => (p === "expanded" ? "collapsed" : "expanded"));
  },
  openDrawer(): void {
    drawerOpenInternal.set(true);
  },
  closeDrawer(): void {
    drawerOpenInternal.set(false);
  },
};
