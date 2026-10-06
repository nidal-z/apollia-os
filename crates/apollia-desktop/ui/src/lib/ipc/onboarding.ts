/**
 * Typed Tauri command wrappers for the first-run onboarding flow.
 *
 * Groups the onboarding lifecycle (phase machine, dismissal, resume) and the
 * embedded acquaintance chat. Components
 * never call `invoke` directly; they
 * import these helpers so the command names and payload shapes live in one
 * typed place (see `crates/apollia-desktop/ui/AGENTS.md`, section 3).
 */
import { invoke } from "@tauri-apps/api/core";
import type {
  ChatSessionDetail,
  OnboardingPhase,
  OnboardingState,
  TriggerResult,
} from "$lib/types";

// ── Lifecycle ──────────────────────────────────────────────────────────────

/** Best-effort advance of the backend phase machine to `target`. */
export async function advanceOnboardingPhase(
  target: OnboardingPhase,
): Promise<void> {
  return invoke<void>("advance_onboarding_phase", { targetPhase: target });
}

/** Persisted onboarding state, used to resume the flow across restarts. */
export async function getOnboardingState(): Promise<OnboardingState> {
  return invoke<OnboardingState>("get_onboarding_state");
}

/** Dismiss onboarding before the chat (skip for now). */
export async function dismissOnboarding(): Promise<void> {
  return invoke<void>("dismiss_onboarding");
}

/** Whether the conversation is over: the agent proposed a profile, or the person skipped. */
export async function checkOnboardingFinalized(): Promise<boolean> {
  return invoke<boolean>("check_onboarding_finalized");
}

/**
 * Finalize the acquaintance chat directly, without a model turn.
 *
 * Stamps the same `onboarding.completed_at` key the review gate reads, so the
 * flow goes straight to the profile form. Backs the "skip the questions"
 * button; the in-chat "finish" button keeps the conversational nudge, which
 * lets the agent propose from what was already said.
 */
export async function finalizeOnboardingChat(): Promise<void> {
  return invoke<void>("finalize_onboarding_chat");
}

// ── Acquaintance chat ────────────────────────────────────────────────────────

/** Start a fresh onboarding chat session. */
export async function triggerOnboarding(): Promise<TriggerResult> {
  return invoke<TriggerResult>("trigger_onboarding", { topic: null });
}

/** Start an onboarding chat that completes an existing profile. */
export async function resumeOnboarding(): Promise<TriggerResult> {
  return invoke<TriggerResult>("resume_onboarding");
}

/** Send a message into an onboarding chat session. */
export async function sendChatMessage(
  sessionId: string,
  content: string,
): Promise<void> {
  return invoke<void>("send_chat_message", { sessionId, content });
}

/** Fetch the full message list for a chat session. */
export async function getChatSession(
  sessionId: string,
): Promise<ChatSessionDetail> {
  return invoke<ChatSessionDetail>("get_chat_session", { sessionId });
}

// ── Completion ───────────────────────────────────────────────────────────────

/** Record that onboarding is done, so the app boots straight to the shell. */
export async function markOnboarded(): Promise<void> {
  return invoke<void>("mark_onboarded");
}
