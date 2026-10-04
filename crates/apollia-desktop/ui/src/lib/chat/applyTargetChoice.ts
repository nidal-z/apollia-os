/**
 * Applies a choice made in the selector of a conversation.
 *
 * `targetAction` says what the choice does; this module does it through the
 * chat commands: the model of the session changes in place, or a new
 * conversation opens on the target, the empty one it replaces removed.
 */
import { get } from "svelte/store";
import { createChatSession } from "$lib/ipc/agents";
import { deleteChatSession, updateChatSession } from "$lib/ipc/chat";
import { chatSessions } from "$lib/stores/sse";
import {
  targetAction,
  type ChatTarget,
  type ConversationTarget,
} from "$lib/chat/chatTargets";
import type { CreateSessionRequest } from "$lib/types";

/** What became of the conversation. */
export interface TargetChoiceOutcome {
  /** The conversation to show now, `null` when the same one stays. */
  openedSessionId: string | null;
}

/** Applies `target`, chosen on the conversation `sessionId`. */
export async function applyTargetChoice(
  target: ChatTarget,
  sessionId: string,
  conversation: ConversationTarget,
  messageCount: number,
): Promise<TargetChoiceOutcome> {
  const action = targetAction(target, conversation, messageCount);
  if (action.kind === "set_model") {
    await updateChatSession(sessionId, { llm_backend: action.route });
    return { openedSessionId: null };
  }
  const request: CreateSessionRequest = { mode: action.mode };
  if (action.agentName) request.agent_name = action.agentName;
  if (action.model) request.model = action.model;
  const session = await createChatSession(request);
  if (action.replace) {
    await deleteChatSession(sessionId).catch(() => {
      /* the empty conversation stays listed; nothing was lost */
    });
    if (get(chatSessions).some((s) => s.id === sessionId)) {
      chatSessions.update((sessions) => sessions.filter((s) => s.id !== sessionId));
    }
  }
  return { openedSessionId: session.id };
}

/**
 * The request that starts a conversation on `target`, chosen before any
 * conversation exists.
 */
export function startRequest(target: ChatTarget | null): CreateSessionRequest {
  if (!target) return { mode: "libre" };
  if (target.kind === "model") return { mode: "libre", model: target.route };
  if (target.builtin) return { mode: "libre" };
  return { mode: "agent", agent_name: target.id };
}
