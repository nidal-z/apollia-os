// ─── Onboarding ───

// ─── Onboarding ───────────────────────────────────────────────────────────────

/** Result of triggering an onboarding session via trigger_onboarding. */
export interface TriggerResult {
  session_id: string;
  mode: "full" | "partial";
  topic?: string;
}

/** Ordered phases of the onboarding flow that the interface reaches. */
export type OnboardingPhase =
  | "welcome"
  | "profile_choice"
  | "ai_setup"
  | "acquaintance"
  | "graduation"
  | "done";

/** Cumulative statistics accumulated across the full onboarding flow. */
export interface OnboardingStats {
  total_time_sec: number;
  actions_completed: number;
  companion_questions: number;
}

/** Full onboarding state returned by get_onboarding_state. */
export interface OnboardingState {
  phase: OnboardingPhase;
  profile: string | null;
  llm_configured: boolean;
  stt_configured: boolean;
  topics_covered: string[];
  mandatory_complete: boolean;
  companion_session_id: string | null;
  voice_enabled: boolean;
  skipped: boolean;
  completed: boolean;
  started_at: string | null;
  completed_at: string | null;
  stats: OnboardingStats;
}
