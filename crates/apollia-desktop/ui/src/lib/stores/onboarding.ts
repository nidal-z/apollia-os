/**
 * Onboarding state store for Apollia Desktop.
 *
 * Tracks whether the user needs onboarding, has skipped it, or completed it.
 * The SSE bridge sets `required` when `OnboardingRequired` arrives from the
 * runtime; the UI sets `skipped` or `completed` based on user actions.
 */
import { writable, derived } from "svelte/store";

interface OnboardingState {
  /** The runtime signaled that onboarding is needed. */
  required: boolean;
  /** The user chose "Plus tard" — dismiss the welcome screen but keep the badge. */
  skipped: boolean;
  /** Onboarding conversation was completed successfully. */
  completed: boolean;
}

function createOnboardingStore() {
  const { subscribe, update, set } = writable<OnboardingState>({
    required: false,
    skipped: false,
    completed: false,
  });

  return {
    subscribe,
    set,

    /** Mark onboarding as required (called by SSE dispatch). */
    setRequired: () => update((s) => ({ ...s, required: true })),

    /** Mark onboarding as skipped (user chose "Plus tard"). */
    setSkipped: () => update((s) => ({ ...s, skipped: true })),

    /** Mark onboarding as completed (conversation finished). */
    setCompleted: () =>
      update((s) => ({ ...s, completed: true, required: false })),
  };
}

export const onboardingStore = createOnboardingStore();

/** Controls the legacy wizard visibility (used by App.svelte). */
export const showOnboarding = writable<boolean>(false);

/** Whether the sidebar badge should be visible. */
export const showOnboardingBadge = derived(
  onboardingStore,
  ($s) => $s.required && $s.skipped && !$s.completed,
);
