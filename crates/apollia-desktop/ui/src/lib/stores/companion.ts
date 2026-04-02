/**
 * Companion panel state store.
 *
 * The Companion is a floating chat panel that provides contextual help on
 * every page. Its position, size, and visibility are persisted in
 * localStorage so they survive page reloads.
 */
import { writable } from "svelte/store";
import { invoke } from "@tauri-apps/api/core";

export interface CompanionPosition {
  x: number;
  y: number;
}

export interface CompanionSize {
  width: number;
  height: number;
}

export interface CompanionState {
  /** Companion panel is open (not hidden). */
  visible: boolean;
  /** Active chat session identifier, `null` when no session has been created. */
  sessionId: string | null;
  /** `true` when the panel is collapsed to a round icon button. */
  minimized: boolean;
  /** Panel position in viewport pixels. `x === -1` means bottom-right auto. */
  position: CompanionPosition;
  /** Panel size in pixels. */
  size: CompanionSize;
  /** Application route currently shown, used to build contextual prompts. */
  currentRoute: string;
}

const STORAGE_KEY = "apollia_companion";

const DEFAULT_COMPANION_STATE: CompanionState = {
  visible: false,
  sessionId: null,
  minimized: false,
  position: { x: -1, y: -1 },
  size: { width: 380, height: 520 },
  currentRoute: "dashboard",
};

/** Clamps a position so the panel stays within the visible viewport. */
function clampPosition(
  pos: CompanionPosition,
  size: CompanionSize,
): CompanionPosition {
  if (pos.x === -1 && pos.y === -1) return pos;
  const vw = window.innerWidth;
  const vh = window.innerHeight;
  return {
    x: Math.max(0, Math.min(pos.x, vw - size.width)),
    y: Math.max(0, Math.min(pos.y, vh - size.height)),
  };
}

/** Reads persisted state from localStorage, falling back to defaults. */
function loadFromStorage(): CompanionState {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return { ...DEFAULT_COMPANION_STATE };
    const parsed = JSON.parse(raw) as Partial<CompanionState>;
    const merged: CompanionState = { ...DEFAULT_COMPANION_STATE, ...parsed };
    // Clamp saved position to the current viewport on load.
    merged.position = clampPosition(merged.position, merged.size);
    return merged;
  } catch {
    return { ...DEFAULT_COMPANION_STATE };
  }
}

/** Writes the current state to localStorage. */
function saveToStorage(state: CompanionState): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(state));
  } catch {
    // Silently ignore quota errors.
  }
}

function createCompanionStore() {
  // Defer localStorage access to avoid SSR issues in tests.
  const initial =
    typeof localStorage !== "undefined"
      ? loadFromStorage()
      : { ...DEFAULT_COMPANION_STATE };

  const { subscribe, update, set } = writable<CompanionState>(initial);

  /** Persist on every mutation. */
  function mutate(fn: (s: CompanionState) => CompanionState): void {
    update((s) => {
      const next = fn(s);
      saveToStorage(next);
      return next;
    });
  }

  return {
    subscribe,

    /** Opens or closes the companion panel. */
    toggleCompanion(): void {
      mutate((s) => ({ ...s, visible: !s.visible, minimized: false }));
    },

    /** Shows the companion panel. */
    openCompanion(): void {
      mutate((s) => ({ ...s, visible: true, minimized: false }));
    },

    /** Hides the companion panel without destroying the session. */
    closeCompanion(): void {
      mutate((s) => ({ ...s, visible: false }));
    },

    /** Collapses the panel to a round icon button. */
    minimizeCompanion(): void {
      mutate((s) => ({ ...s, minimized: true }));
    },

    /** Restores the panel from its collapsed icon state. */
    restoreCompanion(): void {
      mutate((s) => ({ ...s, minimized: false }));
    },

    /** Updates the tracked application route. */
    updateRoute(route: string): void {
      mutate((s) => ({ ...s, currentRoute: route }));
    },

    /** Stores the position after a drag operation, clamping to viewport. */
    setPosition(pos: CompanionPosition): void {
      mutate((s) => ({
        ...s,
        position: clampPosition(pos, s.size),
      }));
    },

    /** Stores the size after a resize, clamped to min/max bounds. */
    setSize(size: CompanionSize): void {
      const clamped: CompanionSize = {
        width: Math.max(300, Math.min(600, size.width)),
        height: Math.max(400, Math.min(800, size.height)),
      };
      mutate((s) => ({ ...s, size: clamped }));
    },

    /** Stores the active session identifier. */
    setSessionId(id: string | null): void {
      mutate((s) => ({ ...s, sessionId: id }));
    },

    /**
     * Creates a new companion chat session via IPC for the given route and
     * stores the returned session id in the companion state.
     */
    async createSession(route?: string): Promise<string> {
      const result = await invoke<{ session_id: string }>(
        "create_companion_session",
        { context: route ?? null },
      );
      const { session_id } = result;
      mutate((s) => ({ ...s, sessionId: session_id }));
      return session_id;
    },

    /** Fetches contextual help text for a route from the backend. */
    async fetchContext(route: string): Promise<string> {
      return invoke<string>("get_companion_context", { route });
    },

    /** Resets the store to defaults and clears localStorage. */
    reset(): void {
      const next = { ...DEFAULT_COMPANION_STATE };
      set(next);
      saveToStorage(next);
    },
  };
}

export const companionStore = createCompanionStore();

/** Whether the floating toggle button should be visible. */
export function isCompanionToggleVisible(state: CompanionState): boolean {
  return state.visible && state.minimized;
}

/** Whether the full companion panel should be rendered. */
export function isCompanionPanelVisible(state: CompanionState): boolean {
  return state.visible && !state.minimized;
}
