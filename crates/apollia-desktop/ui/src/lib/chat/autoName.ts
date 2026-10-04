/**
 * Auto-naming helper for chat sessions.
 *
 * Fires a one-shot LLM call to generate a short title from the user's first
 * message, then optimistically updates the sidebar list so the row swaps
 * "Nouvelle conversation" → generated title without waiting for an SSE refresh.
 *
 * Whether the call is made follows the naming choice of Settings > Chat
 * (`chatAutoNaming`): never, always, or automatically, when the measured
 * generation speed reaches `AUTO_NAME_MIN_TOKENS_PER_SECOND`.
 *
 * Fully fire-and-forget - never blocks the caller, never surfaces failures
 * to the UI (the placeholder title remains usable, manual rename still works).
 */
import { invoke } from "@tauri-apps/api/core";
import { get } from "svelte/store";
import { chatSessions } from "$lib/stores/sse";
import { chatAutoNaming, type ChatAutoNaming } from "$lib/stores/chatAutoNaming";

/**
 * Generation speed, in tokens per second, from which the automatic choice
 * names a conversation. Around comfortable reading speed, the reference the
 * model recommender uses as neither fast nor slow
 * (`REFERENCE_TOKENS_PER_SECOND`, `crates/apollia-llm/src/recommend/rank.rs`):
 * below it, the naming call would hold the engine noticeably while the user
 * waits for the answer.
 */
export const AUTO_NAME_MIN_TOKENS_PER_SECOND = 20;

/**
 * What to do with a first message: name the conversation, skip it, or wait
 * for a first measure of the generation speed (automatic choice only).
 */
export function autoNameDecision(
  choice: ChatAutoNaming,
  tokensPerSecond: number | null,
): "name" | "skip" | "wait" {
  if (choice === "never") return "skip";
  if (choice === "always") return "name";
  if (tokensPerSecond === null) return "wait";
  return tokensPerSecond >= AUTO_NAME_MIN_TOKENS_PER_SECOND ? "name" : "skip";
}

/**
 * Generation speed of one completed model call, from the `LlmCallCompleted`
 * runtime event: generated tokens over generation time. `null` when the call
 * did not measure it.
 */
export function tokensPerSecondOf(call: {
  completion_tokens?: unknown;
  generation_ms?: unknown;
}): number | null {
  const tokens = call.completion_tokens;
  const ms = call.generation_ms;
  if (typeof tokens !== "number" || typeof ms !== "number") return null;
  if (tokens <= 0 || ms <= 0) return null;
  return tokens / (ms / 1000);
}

const attempted = new Set<string>();
const waiting = new Map<string, string>();
let lastTokensPerSecond: number | null = null;

/** The last generation speed measured, `null` before the first. */
export function measuredTokensPerSecond(): number | null {
  return lastTokensPerSecond;
}

/**
 * Records the speed of a completed model call, and settles the conversations
 * the automatic choice held until a speed was known.
 */
export function noteGenerationSpeed(tokensPerSecond: number | null): void {
  if (tokensPerSecond === null || !Number.isFinite(tokensPerSecond)) return;
  lastTokensPerSecond = tokensPerSecond;
  const held = [...waiting];
  waiting.clear();
  for (const [sessionId, firstMessage] of held) {
    attempted.delete(sessionId);
    triggerAutoName(sessionId, firstMessage);
  }
}

/** Reads the speed of every `LlmCallCompleted` on the runtime event channel. */
export async function startGenerationSpeedListener(): Promise<() => void> {
  const { listen } = await import("@tauri-apps/api/event");
  return listen<{ event_type?: string; payload?: Record<string, unknown> }>(
    "runtime-event",
    (event) => {
      if (event.payload.event_type !== "LlmCallCompleted") return;
      const outer = event.payload.payload ?? {};
      const inner = outer.LlmCallCompleted;
      const call = inner && typeof inner === "object" ? inner : outer;
      noteGenerationSpeed(tokensPerSecondOf(call as Record<string, unknown>));
    },
  );
}

/** Forgets every attempt and measure; for tests. */
export function resetAutoNameState(): void {
  attempted.clear();
  waiting.clear();
  lastTokensPerSecond = null;
}

/**
 * Triggers auto-naming for `sessionId` from `firstMessage`. Idempotent:
 * subsequent calls for the same session id are no-ops.
 */
export function triggerAutoName(sessionId: string, firstMessage: string): void {
  if (!sessionId) return;
  const trimmed = firstMessage.trim();
  if (!trimmed) return;
  if (attempted.has(sessionId)) return;

  const decision = autoNameDecision(get(chatAutoNaming), lastTokensPerSecond);
  if (decision === "skip") return;
  attempted.add(sessionId);
  if (decision === "wait") {
    waiting.set(sessionId, trimmed);
    return;
  }

  void (async () => {
    try {
      const title = await invoke<string>("generate_chat_session_name", {
        sessionId,
        firstMessage: trimmed,
      });
      chatSessions.update((sessions) =>
        sessions.map((s) => (s.id === sessionId ? { ...s, title } : s)),
      );
    } catch (err: unknown) {
      // Silent - the placeholder title remains, user can rename manually.
      console.warn("generate_chat_session_name failed:", err);
      // Allow a future retry (e.g. operator types another first message after
      // the LLM error) by clearing the attempted marker.
      attempted.delete(sessionId);
    }
  })();
}
