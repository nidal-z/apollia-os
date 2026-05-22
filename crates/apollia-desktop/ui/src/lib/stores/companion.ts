/**
 * Companion panel state store.
 *
 * The Companion is a floating chat panel that provides contextual help on
 * every page. Panel geometry (position, size, visibility) is persisted in
 * localStorage. The enabled preference is persisted in UserMemory so it
 * survives application reinstalls.
 */
import { writable } from "svelte/store";
import { invoke } from "@tauri-apps/api/core";
import {
  COMPANION_MIN_SIZE,
  clampPosition,
  clampSize,
  validateGeometry,
  type Position,
  type Size,
  type Viewport,
} from "$lib/companion/snapGeometry";

export type CompanionPosition = Position;
export type CompanionSize = Size;

/** Lifecycle stage of the embedded chat session. */
export type CompanionSessionStatus =
  | "idle"
  | "connecting"
  | "creating"
  | "ready";

/** Error category — drives icon, title, and suggested actions. */
export type CompanionErrorKind =
  | "network"
  | "permissions"
  | "timeout"
  | "internal";

export type CompanionErrorCode =
  | "ERR_COMPANION_CONN_TIMEOUT"
  | "ERR_COMPANION_SESSION_FAIL"
  | "ERR_COMPANION_AGENT_UNAVAILABLE"
  | "ERR_COMPANION_UNKNOWN";

export interface CompanionError {
  kind: CompanionErrorKind;
  code: CompanionErrorCode;
  message: string;
  technicalDetails?: string;
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
  /** Lifecycle of the embedded chat session. */
  sessionStatus: CompanionSessionStatus;
  /** Structured error populated when session creation fails. */
  error: CompanionError | null;
  /** Unread message count displayed on the restore button. */
  unreadCount: number;
  /** `true` while the restore button should run its one-shot pulse animation. */
  restorePulseActive: boolean;
}

const STORAGE_KEY = "apollia_companion";
const GEOMETRY_KEY = "companionGeometry";

/** Timeout after which a session-creation attempt falls back to an error. */
export const COMPANION_SESSION_TIMEOUT_MS = 10_000;

const DEFAULT_COMPANION_STATE: CompanionState = {
  enabled: false,
  visible: false,
  sessionId: null,
  minimized: false,
  position: { x: -1, y: -1 },
  size: { width: 380, height: 520 },
  currentRoute: "dashboard",
  currentContext: "",
  sessionStatus: "idle",
  error: null,
  unreadCount: 0,
  restorePulseActive: true,
};

function viewport(): Viewport {
  return { width: window.innerWidth, height: window.innerHeight };
}

function isAutoPosition(pos: CompanionPosition): boolean {
  return pos.x === -1 && pos.y === -1;
}

function safeClampPosition(
  pos: CompanionPosition,
  size: CompanionSize,
): CompanionPosition {
  if (isAutoPosition(pos)) return pos;
  if (typeof window === "undefined") return pos;
  return clampPosition(pos, size, viewport());
}

function safeClampSize(size: CompanionSize): CompanionSize {
  if (typeof window === "undefined") {
    return {
      width: Math.max(COMPANION_MIN_SIZE.width, size.width),
      height: Math.max(COMPANION_MIN_SIZE.height, size.height),
    };
  }
  return clampSize(size, viewport());
}

/**
 * Reads persisted geometry from localStorage, falling back to defaults.
 * Geometry is stored under {@link GEOMETRY_KEY}; legacy state blobs stored
 * under {@link STORAGE_KEY} are still honoured for backwards compatibility.
 */
function loadFromStorage(): CompanionState {
  try {
    const state: CompanionState = { ...DEFAULT_COMPANION_STATE };

    const geoRaw = localStorage.getItem(GEOMETRY_KEY);
    if (geoRaw) {
      const geo = JSON.parse(geoRaw) as {
        position?: Position;
        size?: Size;
      };
      if (geo.size) state.size = geo.size;
      if (geo.position) state.position = geo.position;
    } else {
      const legacy = localStorage.getItem(STORAGE_KEY);
      if (legacy) {
        const parsed = JSON.parse(legacy) as Partial<CompanionState>;
        if (parsed.size) state.size = parsed.size;
        if (parsed.position) state.position = parsed.position;
      }
    }

    if (!isAutoPosition(state.position) && typeof window !== "undefined") {
      const validated = validateGeometry(
        { position: state.position, size: state.size },
        viewport(),
      );
      state.position = validated.position;
      state.size = validated.size;
    } else {
      state.size = safeClampSize(state.size);
    }
    return state;
  } catch {
    return { ...DEFAULT_COMPANION_STATE };
  }
}

/** Writes panel geometry to `localStorage.companionGeometry`. */
function saveGeometry(state: CompanionState): void {
  try {
    const payload = { position: state.position, size: state.size };
    localStorage.setItem(GEOMETRY_KEY, JSON.stringify(payload));
  } catch {
    // Silently ignore quota errors or missing localStorage (SSR/tests).
  }
}

function buildError(err: unknown): CompanionError {
  const message = err instanceof Error ? err.message : String(err);
  const lower = message.toLowerCase();

  if (lower.includes("timeout") || lower.includes("timed out")) {
    return {
      kind: "timeout",
      code: "ERR_COMPANION_CONN_TIMEOUT",
      message,
      technicalDetails: err instanceof Error ? err.stack : undefined,
    };
  }
  if (
    lower.includes("network") ||
    lower.includes("fetch") ||
    lower.includes("connect")
  ) {
    return {
      kind: "network",
      code: "ERR_COMPANION_AGENT_UNAVAILABLE",
      message,
      technicalDetails: err instanceof Error ? err.stack : undefined,
    };
  }
  if (lower.includes("session")) {
    return {
      kind: "internal",
      code: "ERR_COMPANION_SESSION_FAIL",
      message,
      technicalDetails: err instanceof Error ? err.stack : undefined,
    };
  }
  return {
    kind: "internal",
    code: "ERR_COMPANION_UNKNOWN",
    message,
    technicalDetails: err instanceof Error ? err.stack : undefined,
  };
}

async function withTimeout<T>(
  promise: Promise<T>,
  ms: number,
  message: string,
): Promise<T> {
  let timer: ReturnType<typeof setTimeout> | null = null;
  const timeout = new Promise<never>((_, reject) => {
    timer = setTimeout(() => reject(new Error(message)), ms);
  });
  try {
    return await Promise.race([promise, timeout]);
  } finally {
    if (timer) clearTimeout(timer);
  }
}

function createCompanionStore() {
  const initial =
    typeof localStorage !== "undefined"
      ? loadFromStorage()
      : { ...DEFAULT_COMPANION_STATE };

  const { subscribe, update, set } = writable<CompanionState>(initial);

  /** Applies a state mutation; callers that change geometry must persist. */
  function mutate(fn: (s: CompanionState) => CompanionState): void {
    update((s) => fn(s));
  }

  function mutateGeometry(
    fn: (s: CompanionState) => CompanionState,
  ): void {
    update((s) => {
      const next = fn(s);
      saveGeometry(next);
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
      let persistEnabled: boolean | null = null;
      mutate((s) => {
        // If minimized, just restore — don't toggle the enabled preference
        if (s.enabled && s.minimized) {
          return { ...s, minimized: false, unreadCount: 0, restorePulseActive: false };
        }
        const nextEnabled = !s.enabled;
        persistEnabled = nextEnabled;
        return {
          ...s,
          enabled: nextEnabled,
          visible: nextEnabled,
          minimized: false,
        };
      });
      if (persistEnabled !== null) {
        invoke("set_companion_enabled", { enabled: persistEnabled }).catch(() => {});
      }
    },

    /**
     * Toggles visibility without changing the enabled preference.
     * Used by the Cmd+/ (macOS) / Ctrl+/ keyboard shortcut.
     */
    toggleVisibility(): void {
      mutate((s) => ({ ...s, visible: !s.visible, minimized: false }));
    },

    openCompanion(): void {
      mutate((s) => ({ ...s, visible: true, minimized: false }));
    },

    closeCompanion(): void {
      mutate((s) => ({ ...s, visible: false }));
    },

    /** Collapses the panel to a round icon button. */
    minimizeCompanion(): void {
      mutate((s) => ({ ...s, minimized: true }));
    },

    /** Restores the panel from its collapsed icon state. */
    restoreCompanion(): void {
      mutate((s) => ({
        ...s,
        minimized: false,
        unreadCount: 0,
        restorePulseActive: false,
      }));
    },

    /** Increments the unread count shown on the restore button. */
    incrementUnread(): void {
      mutate((s) => ({ ...s, unreadCount: s.unreadCount + 1 }));
    },

    updateRoute(route: string): void {
      mutate((s) => ({ ...s, currentRoute: route }));
    },

    /** Stores the position after a drag operation, clamping to viewport. */
    setPosition(pos: CompanionPosition): void {
      mutateGeometry((s) => ({
        ...s,
        position: safeClampPosition(pos, s.size),
      }));
    },

    /** Stores the size after a resize, clamped to min/max bounds. */
    setSize(size: CompanionSize): void {
      mutateGeometry((s) => {
        const clamped = safeClampSize(size);
        const position = safeClampPosition(s.position, clamped);
        return { ...s, size: clamped, position };
      });
    },

    setSessionId(id: string | null): void {
      mutate((s) => ({ ...s, sessionId: id }));
    },

    /** Manually clears the current error state. */
    clearError(): void {
      mutate((s) => ({ ...s, error: null }));
    },

    /**
     * Creates a new companion chat session via IPC for the given route and
     * stores the returned session id in the companion state. Applies a
     * {@link COMPANION_SESSION_TIMEOUT_MS} wall-clock timeout and maps any
     * failure to a structured {@link CompanionError}.
     */
    async createSession(route?: string): Promise<string> {
      mutate((s) => ({
        ...s,
        sessionStatus: "connecting",
        error: null,
      }));
      try {
        mutate((s) => ({ ...s, sessionStatus: "creating" }));
        const result = await withTimeout(
          invoke<{ session_id: string }>("create_companion_session", {
            context: route ?? null,
          }),
          COMPANION_SESSION_TIMEOUT_MS,
          "Session creation timed out",
        );
        const { session_id } = result;
        mutate((s) => ({
          ...s,
          sessionId: session_id,
          sessionStatus: "ready",
          error: null,
        }));
        return session_id;
      } catch (err) {
        const error = buildError(err);
        mutate((s) => ({ ...s, sessionStatus: "idle", error }));
        throw err;
      }
    },

    async updateContext(route: string): Promise<void> {
      try {
        const text = await invoke<string>("get_companion_context", { route });
        let currentSessionId: string | null = null;
        update((s) => {
          currentSessionId = s.sessionId;
          return { ...s, currentRoute: route, currentContext: text };
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

    async initFromMemory(): Promise<void> {
      try {
        const enabled = await invoke<boolean>("get_companion_enabled");
        if (enabled) {
          // Aligne enabled + visible : si l'user a déjà opt-in, on restaure
          // la companion visible (cohérent avec toggleEnabled qui set les deux
          // en sync). Sans cette ligne, l'user devrait re-cliquer manuellement
          // pour afficher la fenêtre au démarrage.
          mutate((s) => ({ ...s, enabled: true, visible: true }));
        }
      } catch {
        // Non-critical — companion defaults to disabled on error.
      }
    },

    /** Resets the store to defaults and clears localStorage. */
    reset(): void {
      const next = { ...DEFAULT_COMPANION_STATE };
      set(next);
      try {
        localStorage.removeItem(GEOMETRY_KEY);
        localStorage.removeItem(STORAGE_KEY);
      } catch {
        // ignore
      }
    },
  };

  return store;
}

export const companionStore = createCompanionStore();

export function isCompanionRestoreVisible(state: CompanionState): boolean {
  return state.visible && state.minimized;
}

export function isCompanionPanelVisible(state: CompanionState): boolean {
  return state.visible && !state.minimized;
}

/**
 * @deprecated Use {@link isCompanionRestoreVisible}.
 */
export function isCompanionToggleVisible(state: CompanionState): boolean {
  return isCompanionRestoreVisible(state);
}
