/**
 * Companion panel state store.
 *
 * The Companion is a floating chat panel that provides contextual help on
 * every page. Panel geometry (position, size, visibility) is persisted in
 * localStorage. The enabled preference is persisted in UserMemory so it
 * survives application reinstalls.
 */
import { writable, get } from "svelte/store";
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
  /**
   * Whether the companion is enabled by the user (persisted in UserMemory).
   * Initialised to `false`; call `initFromMemory()` on app startup to restore.
   */
  enabled: boolean;
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
  /** Contextual help text for the current route, populated by `updateContext`. */
  currentContext: string;
}

const STORAGE_KEY = "apollia_companion";

const DEFAULT_COMPANION_STATE: CompanionState = {
  enabled: false,
  visible: false,
  sessionId: null,
  minimized: false,
  position: { x: -1, y: -1 },
  size: { width: 380, height: 520 },
  currentRoute: "dashboard",
  currentContext: "",
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

/**
 * Reads persisted geometry from localStorage, falling back to defaults.
 * `enabled` and `currentContext` are always initialised to their defaults
 * so they are never restored from the local cache.
 */
function loadFromStorage(): CompanionState {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return { ...DEFAULT_COMPANION_STATE };
    const parsed = JSON.parse(raw) as Partial<CompanionState>;
    const merged: CompanionState = {
      ...DEFAULT_COMPANION_STATE,
      ...parsed,
      enabled: false,
      currentContext: "",
    };
    merged.position = clampPosition(merged.position, merged.size);
    return merged;
  } catch {
    return { ...DEFAULT_COMPANION_STATE };
  }
}

/**
 * Writes panel geometry to localStorage.
 * `enabled` and `currentContext` are excluded: they are owned by UserMemory
 * and the IPC layer respectively.
 */
function saveToStorage(state: CompanionState): void {
  try {
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    const { enabled: _e, currentContext: _ctx, ...rest } = state;
    localStorage.setItem(STORAGE_KEY, JSON.stringify(rest));
  } catch {
    // Silently ignore quota errors or missing localStorage (SSR/tests).
  }
}

function createCompanionStore() {
  const initial =
    typeof localStorage !== "undefined"
      ? loadFromStorage()
      : { ...DEFAULT_COMPANION_STATE };

  const { subscribe, update, set } = writable<CompanionState>(initial);

  /** Applies a state mutation and persists geometry to localStorage. */
  function mutate(fn: (s: CompanionState) => CompanionState): void {
    update((s) => {
      const next = fn(s);
      saveToStorage(next);
      return next;
    });
  }

  const store = {
    subscribe,

    /**
     * Toggles enabled and visible together (sidebar button action).
     * When enabling, the panel is shown; when disabling, the panel is hidden.
     * The new enabled state is persisted asynchronously to UserMemory.
     */
    toggleCompanion(): void {
      let nextEnabled = false;
      mutate((s) => {
        nextEnabled = !s.enabled;
        return {
          ...s,
          enabled: nextEnabled,
          visible: nextEnabled,
          minimized: false,
        };
      });
      invoke("set_companion_enabled", { enabled: nextEnabled }).catch(() => {});
    },

    /**
     * Toggles visibility without changing the enabled preference.
     * Used by the Cmd+/ (macOS) / Ctrl+/ keyboard shortcut.
     */
    toggleVisibility(): void {
      mutate((s) => ({ ...s, visible: !s.visible, minimized: false }));
    },

    /** Shows the companion panel without changing the enabled preference. */
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

    /** Updates the tracked application route without fetching context. */
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

    /**
     * Fetches the contextual help text for the given route from the backend,
     * stores it in `currentContext`, updates `currentRoute`, and refreshes
     * the active session's system prompt when a session is open.
     */
    async updateContext(route: string): Promise<void> {
      try {
        const text = await invoke<string>("get_companion_context", { route });
        let currentSessionId: string | null = null;
        update((s) => {
          currentSessionId = s.sessionId;
          const next = { ...s, currentRoute: route, currentContext: text };
          saveToStorage(next);
          return next;
        });
        if (currentSessionId) {
          await invoke("update_chat_session", {
            session_id: currentSessionId,
            update: { system_prompt: text, tools: null, llm_backend: null },
          });
        }
      } catch {
        // Non-critical — companion still functions without updated context.
      }
    },

    /**
     * Reads `companion_enabled` from UserMemory and initialises the store.
     * Should be called once at application startup, after the runtime is ready.
     */
    async initFromMemory(): Promise<void> {
      try {
        const entries = await invoke<Array<{ key: string; value: string }>>(
          "get_user_memory",
          { category: "context" },
        );
        const entry = entries.find((e) => e.key === "companion_enabled");
        if (entry?.value === "true") {
          mutate((s) => ({
            ...s,
            enabled: true,
            visible: true,
            minimized: false,
          }));
        }
      } catch {
        // Non-critical — companion defaults to disabled on error.
      }
    },

    /** Resets the store to defaults and clears localStorage. */
    reset(): void {
      const next = { ...DEFAULT_COMPANION_STATE };
      set(next);
      saveToStorage(next);
    },
  };

  return store;
}

export const companionStore = createCompanionStore();

/** Whether the minimized floating restore button should be visible. */
export function isCompanionRestoreVisible(state: CompanionState): boolean {
  return state.visible && state.minimized;
}

/** Whether the full companion panel should be rendered. */
export function isCompanionPanelVisible(state: CompanionState): boolean {
  return state.visible && !state.minimized;
}

/**
 * @deprecated Use {@link isCompanionRestoreVisible}.
 */
export function isCompanionToggleVisible(state: CompanionState): boolean {
  return isCompanionRestoreVisible(state);
}
