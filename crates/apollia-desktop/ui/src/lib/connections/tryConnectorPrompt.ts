/**
 * The "Try" action of the connector wizard's last step.
 *
 * A coaching card carries a ready-made prompt. Trying it opens a free chat
 * limited to the tools of the connector that was just installed, sends the
 * prompt, and routes to that conversation. Nothing is sent, and the wizard
 * stays open, when the connector exposes no tool or the session cannot be
 * created: the operator is told why instead of landing on an empty chat.
 *
 * The decision lives in this module rather than in the route because the
 * frontend runner mounts no Svelte component, so a policy left in the route is
 * out of reach of every guard.
 */
import type { ChatSessionSummary } from "$lib/types";

export interface TryConnectorPromptDeps {
  /** Full registry names of the tools the connector exposes. */
  loadToolNames: () => Promise<string[]>;
  /** Creates a free chat limited to `tools`. Rejects when the backend refused it. */
  createSession: (tools: string[]) => Promise<ChatSessionSummary>;
  /** Sends the prompt as the opening message of the session. */
  sendMessage: (sessionId: string, prompt: string) => Promise<unknown>;
  /** Starts naming the session from its opening prompt. */
  nameSession: (sessionId: string, prompt: string) => void;
  /** Hands the session id to the chat route. */
  rememberSession: (sessionId: string) => void;
  /** Surfaces the failure where the operator is looking. */
  report: (err: unknown) => void;
  /** Routes to the chat screen. Reached only once a session exists. */
  navigate: () => void;
  /** Localised text reported when the connector exposes no tool. */
  noToolsMessage: string;
}

/**
 * Returns `true` when the session exists and the route changed, `false` when
 * the failure was reported and the operator stayed where they were.
 */
export async function tryConnectorPrompt(
  prompt: string,
  deps: TryConnectorPromptDeps,
): Promise<boolean> {
  let session: ChatSessionSummary;
  try {
    const tools = await deps.loadToolNames();
    if (tools.length === 0) {
      deps.report(new Error(deps.noToolsMessage));
      return false;
    }
    session = await deps.createSession(tools);
  } catch (err) {
    deps.report(err);
    return false;
  }
  deps.nameSession(session.id, prompt);
  // A failed send leaves an empty session the operator can type into; it must
  // not block the route change.
  await deps.sendMessage(session.id, prompt).catch(() => {});
  deps.rememberSession(session.id);
  deps.navigate();
  return true;
}
