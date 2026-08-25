/**
 * Typed Tauri command wrappers for the first-run onboarding flow.
 *
 * Groups the onboarding lifecycle (phase machine, dismissal, resume), the
 * embedded acquaintance chat, the proposed-permission triage, and the
 * post-onboarding guided tour. Components never call `invoke` directly; they
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

/** A single permission-rule proposal emitted under `onboarding.proposed_rules`. */
export interface ProposedRuleView {
  index: number;
  tool_name: string;
  action: string;
  arg_prefix: string | null;
  scope: string;
}

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

/** Whether the agent has written the authoritative `onboarding.completed_at`. */
export async function checkOnboardingFinalized(): Promise<boolean> {
  return invoke<boolean>("check_onboarding_finalized");
}

/**
 * Finalize the acquaintance chat directly, without a model turn.
 *
 * Stamps the same `onboarding.completed_at` key the wrap-up gate reads, so the
 * flow routes straight to the permission proposals. Backs the "skip the
 * optional questions" button; the in-chat "finish" button keeps the
 * conversational nudge instead.
 */
export async function finalizeOnboardingChat(): Promise<void> {
  return invoke<void>("finalize_onboarding_chat");
}

// ── Acquaintance chat ────────────────────────────────────────────────────────

/** Start a fresh onboarding chat session. */
export async function triggerOnboarding(): Promise<TriggerResult> {
  return invoke<TriggerResult>("trigger_onboarding", {
    topic: null,
    profile: null,
  });
}

/** Resume an onboarding chat, keeping the profile already collected. */
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

// ── Proposed permission triage ───────────────────────────────────────────────

export async function listProposedPermissionRules(): Promise<ProposedRuleView[]> {
  return invoke<ProposedRuleView[]>("list_proposed_permission_rules");
}

export async function applyProposedPermissionRule(index: number): Promise<void> {
  return invoke<void>("apply_proposed_permission_rule", { index });
}

export async function dismissProposedPermissionRule(
  index: number,
): Promise<void> {
  return invoke<void>("dismiss_proposed_permission_rule", { index });
}

/** Record that onboarding is done, so the app boots straight to the shell. */
export async function markOnboarded(): Promise<void> {
  return invoke<void>("mark_onboarded");
}
