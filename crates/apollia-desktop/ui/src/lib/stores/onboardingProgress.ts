/**
 * `onboardingProgress` — debounced localStorage-backed saver for in-flight
 * onboarding answers. Lets the user resume where they left off after a reload
 * without round-tripping to the backend (see TODO below).
 *
 * TODO : persist progress server-side via
 *   GET /settings/onboarding_state and PUT /settings/onboarding_state once
 *   the backend endpoints land. For now this is a client-only fallback.
 *
 * Story:.
 */

/** localStorage key used to persist onboarding progress. */
export const PROGRESS_KEY = "apollia.onboarding.progress";

/** Debounce window in milliseconds applied to `saveOnboardingProgress`. */
export const DEBOUNCE_MS = 500;

/** Shape of the persisted progress blob. */
export interface OnboardingProgress {
  /** Identifier of the current step within the onboarding flow. */
  currentStep: string;
  /** User-provided answers so far (free-form key/value). */
  answers: Record<string, unknown>;
  /** Epoch milliseconds of the last save. */
  savedAt?: number;
}

let timer: ReturnType<typeof setTimeout> | null = null;
let pending: OnboardingProgress | null = null;

function flush(): void {
  if (pending === null) return;
  try {
    const payload: OnboardingProgress = { ...pending, savedAt: Date.now() };
    localStorage.setItem(PROGRESS_KEY, JSON.stringify(payload));
  } catch {
    // Storage quota or disabled — silently drop the save.
  }
  pending = null;
  timer = null;
}

/**
 * Queue a progress save. Consecutive calls within {@link DEBOUNCE_MS} collapse
 * into a single localStorage write.
 */
export function saveOnboardingProgress(progress: {
  currentStep: string;
  answers: Record<string, unknown>;
}): void {
  pending = { currentStep: progress.currentStep, answers: progress.answers };
  if (timer !== null) clearTimeout(timer);
  timer = setTimeout(flush, DEBOUNCE_MS);
}

/** Load the last persisted progress, or `null` if absent / corrupted. */
export function loadOnboardingProgress(): OnboardingProgress | null {
  try {
    const raw = localStorage.getItem(PROGRESS_KEY);
    if (raw === null) return null;
    const parsed = JSON.parse(raw) as unknown;
    if (
      typeof parsed === "object" &&
      parsed !== null &&
      typeof (parsed as OnboardingProgress).currentStep === "string"
    ) {
      return parsed as OnboardingProgress;
    }
    return null;
  } catch {
    return null;
  }
}

/** Clear any persisted progress. Useful on completion/reset. */
export function clearOnboardingProgress(): void {
  try {
    localStorage.removeItem(PROGRESS_KEY);
  } catch {
    // No-op.
  }
}
