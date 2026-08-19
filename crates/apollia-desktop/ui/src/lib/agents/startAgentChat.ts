/**
 * The "chat with this agent" action of the agents route.
 *
 * The button creates a chat session bound to the agent, hands its id to the
 * chat route through `pendingChatSessionId`, then navigates. Navigating on a
 * failed creation dropped the operator onto an empty conversation with no
 * agent behind it and no explanation, because the id the chat route waits for
 * was never produced.
 *
 * The decision lives in this module rather than inside the route because the
 * frontend runner mounts no Svelte component (`vitest.config.ts` is node-only
 * and collects `.test.ts` files), so a policy left in the route is out of
 * reach of every guard this repository has.
 */
import type { ChatSessionSummary } from "$lib/types";

export interface StartAgentChatDeps {
  /** Creates the agent-bound session. Rejects when the backend refused it. */
  createSession: () => Promise<ChatSessionSummary>;
  /** Hands the session id to the chat route. */
  rememberSession: (sessionId: string) => void;
  /** Surfaces the failure where the operator is looking. */
  report: (err: unknown) => void;
  /** Routes to the chat screen. Reached only once a session exists. */
  navigate: () => void;
}

/**
 * Open a conversation with an agent, and route to it only if it exists.
 *
 * Returns `true` when the session was created and the route changed, `false`
 * when the failure was reported and the operator stayed where they were.
 */
export async function startAgentChat(
  deps: StartAgentChatDeps,
): Promise<boolean> {
  let session: ChatSessionSummary;
  try {
    session = await deps.createSession();
  } catch (err) {
    deps.report(err);
    return false;
  }
  deps.rememberSession(session.id);
  deps.navigate();
  return true;
}
