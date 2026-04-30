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

const DEFAULT_STATE: OnboardingState = {
  phase: "welcome",
  profile: null,
  llm_configured: false,
  stt_configured: false,
  topics_covered: [],
  mandatory_complete: false,
  tour_step_index: 0,
  tour_total_steps: 0,
  tour_completed: false,
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
    voice_commands_used: 0,
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

    /**
     * Persist the chosen profile (`"operator"` or `"builder"`) via IPC,
     * then update the store with the returned state.
     */
    async setProfile(profile: string): Promise<void> {
      const newState = await invoke<OnboardingState>("set_onboarding_profile", { profile });
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

/** Whether the sidebar onboarding badge should be visible. */
export const showOnboardingBadge = derived(
  onboardingStore,
  ($s) => $s.skipped && !$s.completed,
);
