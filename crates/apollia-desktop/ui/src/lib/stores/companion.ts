/**
 * Guide state store.
 *
 * The guide lives in the Guide tab of the detail sidebar
 * (`$lib/stores/detailSidebar`), which owns whether it is shown. This store
 * holds the guide's chat session and the context it was given: the route,
 * the sub-screen the screen declares and the object it shows
 * (`$lib/stores/guideContext`). The enabled preference is persisted in
 * UserMemory so it survives application reinstalls.
 */
import { get, writable } from "svelte/store";
import { invoke } from "@tauri-apps/api/core";
import { detailSidebar, guideVisible } from "$lib/stores/detailSidebar";
import {
  guidePromptDetails,
  type GuideContext,
  type GuideObject,
  type GuideSubscreen,
} from "$lib/stores/guideContext";

/** Lifecycle stage of the embedded chat session. */
export type CompanionSessionStatus =
  | "idle"
  | "connecting"
  | "creating"
  | "ready";

/** Error category - drives icon, title, and suggested actions. */
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
  /** Active chat session identifier, `null` when no session has been created. */
  sessionId: string | null;
  /** Application route the guide was last given, or `onboarding`. */
  currentRoute: string;
  /** Sub-screen the screen declared, `null` on the screen itself. */
  currentSubscreen: GuideSubscreen | null;
  /** Object the sub-screen shows, `null` when none. */
  currentObject: GuideObject | null;
  /** Instructions given to the guide for the current context. */
  currentContext: string;
  /** Lifecycle of the embedded chat session. */
  sessionStatus: CompanionSessionStatus;
  /** Structured error populated when session creation fails. */
  error: CompanionError | null;
}

/** Timeout after which a session-creation attempt falls back to an error. */
export const COMPANION_SESSION_TIMEOUT_MS = 10_000;

const DEFAULT_COMPANION_STATE: CompanionState = {
  enabled: false,
  sessionId: null,
  currentRoute: "dashboard",
  currentSubscreen: null,
  currentObject: null,
  currentContext: "",
  sessionStatus: "idle",
  error: null,
};

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
  const { subscribe, update, set } = writable<CompanionState>({
    ...DEFAULT_COMPANION_STATE,
  });

  function mutate(fn: (s: CompanionState) => CompanionState): void {
    update((s) => fn(s));
  }

  const store = {
    subscribe,

    /**
     * Opens the Guide tab, or collapses it when it is already shown (palette
     * action). The resulting visibility is persisted to UserMemory as the
     * enabled preference.
     */
    toggleCompanion(): void {
      detailSidebar.toggleGuide();
      const enabled = get(guideVisible);
      mutate((s) => ({ ...s, enabled }));
      invoke("set_companion_enabled", { enabled }).catch(() => {});
    },

    /** Opens the detail sidebar on the Guide tab. */
    openCompanion(): void {
      detailSidebar.openGuide();
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

    /**
     * Gives the guide the context of the screen: the route's text, followed
     * by the declared sub-screen and the object it shows. The text is looked
     * up by route; the sub-screen and the object reach the guide through the
     * lines `guidePromptDetails` appends.
     */
    async updateContext(context: GuideContext): Promise<void> {
      try {
        const routeText = await invoke<string>("get_companion_context", {
          route: context.route,
        });
        const details = guidePromptDetails(context);
        const text = details ? `${routeText}\n\n${details}` : routeText;
        let currentSessionId: string | null = null;
        update((s) => {
          currentSessionId = s.sessionId;
          return {
            ...s,
            currentRoute: context.route,
            currentSubscreen: context.subscreen,
            currentObject: context.object,
            currentContext: text,
          };
        });
        if (currentSessionId) {
          // Tauri camel-cases the names of command arguments before looking
          // them up, so the key must be `sessionId`. The nested `update`
          // payload keeps snake_case: `UpdateSessionRequest` carries no
          // `rename_all`, and serde reads its fields as declared.
          await invoke("update_chat_session", {
            sessionId: currentSessionId,
            update: { system_prompt: text, tools: null, llm_backend: null },
          });
        }
      } catch {
        // Non-critical - companion still functions without updated context.
      }
    },

    async initFromMemory(): Promise<void> {
      try {
        const enabled = await invoke<boolean>("get_companion_enabled");
        if (enabled) {
          // Restore the opt-in but keep the Guide tab closed at startup.
          // Opening it on launch spawns an empty chat session every time; the
          // user opens the guide on demand from the help icon or Cmd+/.
          mutate((s) => ({ ...s, enabled: true }));
        }
      } catch {
        // Non-critical - companion defaults to disabled on error.
      }
    },

    /** Resets the store to defaults. */
    reset(): void {
      set({ ...DEFAULT_COMPANION_STATE });
    },
  };

  return store;
}

export const companionStore = createCompanionStore();
