/**
 * Layout store — source de vérité pour l'état du sidebar et du companion.
 *
 * Dérive `sidebarState` de la viewport via `window.matchMedia` :
 *   - ≥ `lg` (1024 px) → `expanded` ou `icon` selon la préférence utilisateur
 * - `md`–`lg` → `icon` forcé (auto-collapse E.17)
 *   - < `md`          → `drawer` overlay (E.37)
 *
 * La préférence `expanded` / `collapsed` à `lg+` est persistée dans
 * `localStorage` sous la clé `apollia.ui.sidebar` (AC).
 */
import { derived, get, readable, writable, type Readable } from "svelte/store";
import { companionStore } from "./companion";

/** État effectif de la sidebar à un instant donné. */
export type SidebarState = "expanded" | "icon" | "drawer";

/** Préférence utilisateur persistée. Ne s'applique qu'à `lg+`. */
type SidebarPreference = "expanded" | "collapsed";

type Viewport = "sm" | "md" | "lg";

// Legacy single-key storage — migrated on first load.
const LEGACY_KEY = "apollia.ui.sidebar";
// Per-breakpoint key prefix. Values : `open | collapsed | hidden`.
const STATE_KEY_PREFIX = "apollia.ui.sidebarState_";
// Breakpoints canoniques — voir `src/lib/design/breakpoints.md`.
const MD_QUERY = "(min-width: 768px)";
const LG_QUERY = "(min-width: 1024px)";

function computeViewport(): Viewport {
  if (typeof window === "undefined") return "lg";
  if (window.matchMedia(LG_QUERY).matches) return "lg";
  if (window.matchMedia(MD_QUERY).matches) return "md";
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
    // Quota or disabled storage — silently ignore.
  }
}

function loadPreference(): SidebarPreference {
  migrateLegacy();
  return loadState("lg") === "collapsed" ? "collapsed" : "expanded";
}

function savePreference(pref: SidebarPreference): void {
  saveState("lg", pref === "collapsed" ? "collapsed" : "open");
}

/** Store read-only de la viewport courante, mis à jour via `matchMedia`. */
const viewport: Readable<Viewport> = readable<Viewport>(computeViewport(), (set) => {
  if (typeof window === "undefined") return;
  const mdMql = window.matchMedia(MD_QUERY);
  const lgMql = window.matchMedia(LG_QUERY);
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

// Persist drawer open state under sm — `hidden` when closed, `open` when open.
drawerOpenInternal.subscribe((open) => {
  if (typeof window === "undefined") return;
  if (computeViewport() !== "sm") return;
  saveState("sm", open ? "open" : "hidden");
});

/**
 * État sidebar dérivé de la viewport et de la préférence utilisateur.
 * Mobile-first : toute viewport < `md` force `drawer`.
 */
export const sidebarState: Readable<SidebarState> = derived(
  [viewport, preference],
  ([$v, $p]) => {
    if ($v === "sm") return "drawer";
    if ($v === "md") return "icon";
    return $p === "collapsed" ? "icon" : "expanded";
  },
);

/** `true` quand la sidebar est rendue en overlay ET ouverte. */
export const drawerOpen: Readable<boolean> = derived(
  [sidebarState, drawerOpenInternal],
  ([$s, $o]) => $s === "drawer" && $o,
);

/** Mirror du companion pour observer sidebar + companion ensemble. */
export const companionOpen: Readable<boolean> = derived(
  companionStore,
  ($c) => $c.visible && !$c.minimized,
);

/** Vue agrégée — utile pour les tests / la télémétrie / le debug. */
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
 * Actions réactives sur le layout.
 * - `toggleSidebar` : collapse/expand à `lg+`, ouvre/ferme le drawer sous `md`,
 *   no-op entre `md` et `lg` (icon-only forcé).
 */
export const layoutActions = {
  toggleSidebar(): void {
    const v = get(viewport);
    if (v === "sm") {
      drawerOpenInternal.update((o) => !o);
      return;
    }
    if (v === "md") {
      // icon-only forcé entre md et lg — pas de préférence utilisateur.
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
