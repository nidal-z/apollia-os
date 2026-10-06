/**
 * Onboarding state store for Apollia Desktop.
 *
 * Mirrors the backend OnboardingState (7-phase machine persisted in
 * UserMemory). All mutations go through IPC so the backend remains the
 * single source of truth. Local-only helpers from the previous store
 * (setRequired, setSkipped) are preserved for backward compatibility with
 * the SSE dispatcher and Settings route.
 */
import { writable, derived } from "svelte/store";
import { invoke } from "@tauri-apps/api/core";
import type { OnboardingState, OnboardingPhase } from "$lib/types";
import type { EnrichmentRequest, PendingProposal } from "$lib/onboarding/profileProposal";
import { parseFirstAgent, type FirstAgent } from "$lib/onboarding/firstAgent";

const DEFAULT_STATE: OnboardingState = {
  phase: "welcome",
  llm_configured: false,
  stt_configured: false,
  topics_covered: [],
  mandatory_complete: false,
  companion_session_id: null,
  voice_enabled: false,
  skipped: false,
  completed: false,
  started_at: null,
  completed_at: null,
  stats: {
    total_time_sec: 0,
    actions_completed: 0,
    companion_questions: 0,
  },
};

function createOnboardingStore() {
  const { subscribe, update, set } = writable<OnboardingState>(DEFAULT_STATE);

  return {
    subscribe,
    set,

    /** Fetch the current state from the backend and update the store. */
    async refreshState(): Promise<void> {
      const state = await invoke<OnboardingState>("get_onboarding_state");
      set(state);
    },

    /**
     * Advance to `targetPhase` via IPC, then update the store with the
     * returned state. Throws on invalid transitions or backend errors.
     */
    async advancePhase(targetPhase: OnboardingPhase): Promise<void> {
      const newState = await invoke<OnboardingState>("advance_onboarding_phase", { targetPhase });
      set(newState);
    },

    // ── Legacy helpers (backward compat with SSE dispatcher and routes) ──────

    /**
     * Called by the SSE dispatcher when an `onboarding-required` event
     * arrives. Refreshes backend state so phase-based routing takes effect.
     */
    setRequired(): void {
      void invoke<OnboardingState>("get_onboarding_state")
        .then(set)
        .catch(() => {});
    },

    /**
     * Called when the user dismisses the onboarding reminder badge without
     * completing the flow. Marks the store locally so the badge disappears.
     */
    setSkipped(): void {
      update((s) => ({ ...s, skipped: true }));
    },
  };
}

export const onboardingStore = createOnboardingStore();

/**
 * Controls visibility of the agent-driven onboarding modal.
 *
 * Set to `true` by:
 *  - `App.svelte` on first-launch detection (state has no `started_at`),
 *  - the `runtime-event` listener when the supervisor emits
 *    `OnboardingRequired` (category `onboarding-changed`),
 *  - the command palette ("Restart onboarding" action).
 *
 * Set to `false` by the modal itself when the user dismisses it or when
 * `OnboardingCompleted` is emitted.
 */
export const onboardingModalOpen = writable(false);

/**
 * The profile the next onboarding conversation is asked to complete, or
 * `null` for the first-launch conversation. Set by the "complete this profile"
 * banner of Settings > Profile with the displayed profile; the chat step
 * takes it (and clears it) when the session starts, calls `resume_onboarding`
 * instead of `trigger_onboarding`, and tells the agent what the profile
 * already holds.
 */
export const profileEnrichment = writable<EnrichmentRequest | null>(null);

/**
 * The proposal a completed enrichment conversation hands back to Settings >
 * Profile. Nothing is written when it is set: the page lays it over the form
 * of the profile it was asked for, where the person reads it over and saves
 * it or not, then clears it.
 */
export const pendingProfileProposal = writable<PendingProposal | null>(null);

/** The `localStorage` key the agent of the first launch is kept under. */
export const FIRST_AGENT_STORAGE_KEY = "apollia.onboarding.first_agent";

function readStoredFirstAgent(): FirstAgent | null {
  if (typeof localStorage === "undefined") return null;
  try {
    return parseFirstAgent(localStorage.getItem(FIRST_AGENT_STORAGE_KEY));
  } catch {
    return null;
  }
}

function storeFirstAgent(agent: FirstAgent | null): void {
  if (typeof localStorage === "undefined") return;
  try {
    if (agent === null) localStorage.removeItem(FIRST_AGENT_STORAGE_KEY);
    else localStorage.setItem(FIRST_AGENT_STORAGE_KEY, JSON.stringify(agent));
  } catch {
    // Storage refused: the agent is kept for this run of the application only.
  }
}

function createFirstAgentStore() {
  const { subscribe, set } = writable<FirstAgent | null>(readStoredFirstAgent());

  return {
    subscribe,

    /** Keeps `agent`, obtained at the "first agent" step, for the next steps. */
    retain(agent: FirstAgent): void {
      storeFirstAgent(agent);
      set(agent);
    },

    /**
     * Keeps no agent: the step was passed, or the first launch is replayed
     * from its start. The steps after it then propose no connection and open
     * the first conversation with Apollia.
     */
    clear(): void {
      storeFirstAgent(null);
      set(null);
    },
  };
}

/**
 * The agent the "first agent" step of the first launch ended with, `null`
 * when the step was passed or not reached yet. The connection step and the
 * first-task step read it, through `$lib/onboarding/firstAgent`. Kept across
 * a restart, so a first launch resumed on a later step still knows it.
 */
export const onboardingFirstAgent = createFirstAgentStore();

/** Whether the sidebar onboarding badge should be visible. */
export const showOnboardingBadge = derived(
  onboardingStore,
  ($s) => $s.skipped && !$s.completed,
);
