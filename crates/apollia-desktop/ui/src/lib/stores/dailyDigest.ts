/**
 * Daily digest store for the operator Dashboard.
 *
 * Facts are computed frontend-side from existing stores (tasks, approvals).
 * The backend `meta_generate_daily_digest` command caches the narration for
 * 6h; `refresh()` bypasses the cache. Narration will switch to the Meta-LLM
 * path without any contract change here — see the `fromLlm`
 * field on the payload.
 */
import { writable, get } from "svelte/store";
import { invoke } from "@tauri-apps/api/core";

export interface DigestFacts {
  tasksCompleted: number;
  tasksFailed: number;
  approvalsPending: number;
  insightsCreated: number;
  automationsFailed: number;
  firstFailureLabel?: string | null;
  topAssistant?: string | null;
}

export interface DailyDigest {
  narration: string;
  sourceFacts: DigestFacts;
  generatedAt: string;
  cached: boolean;
  fromLlm: boolean;
}

interface TauriResponse {
  narration: string;
  sourceFacts: DigestFacts;
  generatedAt: string;
  cached: boolean;
  fromLlm: boolean;
}

interface State {
  digest: DailyDigest | null;
  loading: boolean;
  error: string | null;
}

const initial: State = { digest: null, loading: false, error: null };

function createStore() {
  const { subscribe, update, set } = writable<State>(initial);

  async function fetchDigest(facts: DigestFacts, forceRefresh: boolean) {
    update((s) => ({ ...s, loading: true, error: null }));
    try {
      const now = new Date();
      const userDate = `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, "0")}-${String(now.getDate()).padStart(2, "0")}`;
      const response = await invoke<TauriResponse>("meta_generate_daily_digest", {
        request: {
          userDate,
          windowHours: 24,
          facts,
          forceRefresh,
        },
      });
      update((s) => ({ ...s, digest: response, loading: false }));
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      update((s) => ({ ...s, loading: false, error: msg }));
    }
  }

  return {
    subscribe,
    /** Load the digest using current facts (cache-honoring). */
    load(facts: DigestFacts) {
      void fetchDigest(facts, false);
    },
    /** Force regeneration — costs one LLM call when 057 is wired. */
    refresh(facts: DigestFacts) {
      void fetchDigest(facts, true);
    },
    /** Current snapshot, primarily for tests. */
    peek() {
      return get({ subscribe });
    },
    reset() {
      set(initial);
    },
  };
}

export const dailyDigest = createStore();
