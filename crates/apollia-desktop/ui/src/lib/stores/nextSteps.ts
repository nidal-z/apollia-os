/**
 * Next Steps store (US-SP42-059).
 *
 * Powers the `NextStepsPanel` on two surfaces:
 *   - `global` — operator Dashboard, rolling context of the last ~12h.
 *   - `session:<id>` — end-of-session debrief on `ChatConversation`.
 *
 * Responsibilities:
 *   - Cache the Meta-LLM response in-memory per scope. Global is refreshed
 *     at most every 12h; session panels are one-shot (refreshed on user
 *     request or when a new close event fires).
 *   - Persist dismissed card ids in `localStorage` for 24h so a card the
 *     user actively dismissed does not come back on the next mount.
 *   - Persist inline thumbs-up / thumbs-down feedback per card id (local
 *     only — never uploaded).
 */

import { writable, get, derived, type Readable } from "svelte/store";
import { invoke } from "@tauri-apps/api/core";

// ────────────────────────── Types (mirror backend) ──────────────────────────

export type NextStepAction = "navigate" | "invoke";

export interface NextStepButton {
  label: string;
  action: NextStepAction;
  payload: Record<string, unknown>;
}

export interface NextStep {
  id: string;
  title: string;
  description: string;
  actionButton: NextStepButton;
}

export interface NextStepsFacts {
  recentMessages?: string[];
  toolsUsed?: string[];
  memoriesCreated?: number;
  memoriesRecalled?: number;
  inboxPending?: number;
  tasksFailed?: number;
  tasksCompleted?: number;
  automationsFailing?: number;
  signals?: string[];
}

export type NextStepsContext = "global_context" | "session_end";
export type NextStepsMode = "operator" | "builder";

interface BackendResponse {
  steps: NextStep[];
  fromLlm: boolean;
}

interface CachedScope {
  steps: NextStep[];
  fromLlm: boolean;
  generatedAt: number; // epoch ms
  loading: boolean;
  error: string | null;
}

type Feedback = "useful" | "not_useful";

// ─────────────────────────── Persistence helpers ───────────────────────────

const DISMISS_KEY = "apollia.next_steps.dismissed";
const FEEDBACK_KEY = "apollia.next_steps.feedback";
const DISMISS_TTL_MS = 24 * 3600 * 1000;

/** `{ [cardId]: expiresAtEpochMs }` */
type DismissMap = Record<string, number>;
/** `{ [cardId]: "useful" | "not_useful" }` */
type FeedbackMap = Record<string, Feedback>;

function readDismissMap(): DismissMap {
  if (typeof localStorage === "undefined") return {};
  try {
    const raw = localStorage.getItem(DISMISS_KEY);
    if (!raw) return {};
    const parsed = JSON.parse(raw) as DismissMap;
    const now = Date.now();
    let changed = false;
    for (const key of Object.keys(parsed)) {
      if (parsed[key] <= now) {
        delete parsed[key];
        changed = true;
      }
    }
    if (changed) localStorage.setItem(DISMISS_KEY, JSON.stringify(parsed));
    return parsed;
  } catch {
    return {};
  }
}

function writeDismissMap(map: DismissMap): void {
  if (typeof localStorage === "undefined") return;
  try {
    localStorage.setItem(DISMISS_KEY, JSON.stringify(map));
  } catch {
    /* quota exceeded — ignore */
  }
}

function readFeedbackMap(): FeedbackMap {
  if (typeof localStorage === "undefined") return {};
  try {
    const raw = localStorage.getItem(FEEDBACK_KEY);
    return raw ? (JSON.parse(raw) as FeedbackMap) : {};
  } catch {
    return {};
  }
}

function writeFeedbackMap(map: FeedbackMap): void {
  if (typeof localStorage === "undefined") return;
  try {
    localStorage.setItem(FEEDBACK_KEY, JSON.stringify(map));
  } catch {
    /* ignore */
  }
}

// ─────────────────────────── Store ───────────────────────────

const GLOBAL_TTL_MS = 12 * 3600 * 1000;

function initialScope(): CachedScope {
  return {
    steps: [],
    fromLlm: false,
    generatedAt: 0,
    loading: false,
    error: null,
  };
}

type ScopeMap = Record<string, CachedScope>;

function createStore() {
  const scopes = writable<ScopeMap>({});
  const dismissed = writable<DismissMap>(readDismissMap());
  const feedback = writable<FeedbackMap>(readFeedbackMap());

  function setScope(scopeKey: string, patch: Partial<CachedScope>) {
    scopes.update((s) => ({
      ...s,
      [scopeKey]: { ...(s[scopeKey] ?? initialScope()), ...patch },
    }));
  }

  function peekScope(scopeKey: string): CachedScope {
    return get(scopes)[scopeKey] ?? initialScope();
  }

  async function fetchSteps(
    scopeKey: string,
    context: NextStepsContext,
    mode: NextStepsMode,
    facts: NextStepsFacts,
    forceRefresh: boolean,
  ) {
    const existing = peekScope(scopeKey);
    const fresh =
      context === "global_context" &&
      existing.steps.length > 0 &&
      Date.now() - existing.generatedAt < GLOBAL_TTL_MS;
    if (!forceRefresh && fresh) return;

    setScope(scopeKey, { loading: true, error: null });
    try {
      const response = await invoke<BackendResponse>("meta_generate_next_steps", {
        request: { context, mode, facts },
      });
      setScope(scopeKey, {
        steps: response.steps,
        fromLlm: response.fromLlm,
        generatedAt: Date.now(),
        loading: false,
        error: null,
      });
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      setScope(scopeKey, { loading: false, error: msg });
    }
  }

  /** Visible (non-dismissed) cards for the given scope. */
  function visible(scopeKey: string): Readable<{
    steps: NextStep[];
    fromLlm: boolean;
    loading: boolean;
    error: string | null;
  }> {
    return derived([scopes, dismissed], ([$scopes, $dismissed]) => {
      const sc = $scopes[scopeKey] ?? initialScope();
      const steps = sc.steps.filter((s) => !$dismissed[s.id]);
      return {
        steps,
        fromLlm: sc.fromLlm,
        loading: sc.loading,
        error: sc.error,
      };
    });
  }

  function dismiss(cardId: string) {
    dismissed.update((m) => {
      const next = { ...m, [cardId]: Date.now() + DISMISS_TTL_MS };
      writeDismissMap(next);
      return next;
    });
  }

  function setFeedback(cardId: string, value: Feedback) {
    feedback.update((m) => {
      const next = { ...m, [cardId]: value };
      writeFeedbackMap(next);
      return next;
    });
  }

  function feedbackFor(cardId: string): Feedback | undefined {
    return get(feedback)[cardId];
  }

  function reset() {
    scopes.set({});
  }

  return {
    scopes: { subscribe: scopes.subscribe },
    dismissed: { subscribe: dismissed.subscribe },
    feedback: { subscribe: feedback.subscribe },
    load(
      scopeKey: string,
      context: NextStepsContext,
      mode: NextStepsMode,
      facts: NextStepsFacts,
    ) {
      void fetchSteps(scopeKey, context, mode, facts, false);
    },
    refresh(
      scopeKey: string,
      context: NextStepsContext,
      mode: NextStepsMode,
      facts: NextStepsFacts,
    ) {
      void fetchSteps(scopeKey, context, mode, facts, true);
    },
    visible,
    dismiss,
    setFeedback,
    feedbackFor,
    peekScope,
    reset,
  };
}

export const nextSteps = createStore();

/** Key for the operator Dashboard panel — single global scope. */
export const GLOBAL_SCOPE = "global";
/** Helper to compute a stable per-session scope key. */
export function sessionScope(sessionId: string): string {
  return `session:${sessionId}`;
}
