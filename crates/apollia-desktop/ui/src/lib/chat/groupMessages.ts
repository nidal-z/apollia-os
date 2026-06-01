import type { ChatMessageView } from "$lib/types";

export interface MessageGroup {
  /** Role of every message in the group - groups never mix roles. */
  role: ChatMessageView["role"];
  /** Stable key derived from first message id - useful for keyed #each. */
  key: string;
  /** Messages in chronological order. */
  messages: ChatMessageView[];
  /** Timestamp of the first message - used for the group header. */
  startedAt: string;
}

/** Upper bound on the delta between two messages of the same role to stay grouped. */
export const GROUP_WINDOW_MS = 5 * 60 * 1000;

/** Roles that accept grouping. system/tool stay standalone. */
const GROUPABLE_ROLES = new Set<ChatMessageView["role"]>(["user", "assistant"]);

function hasToolCalls(msg: ChatMessageView): boolean {
  return Array.isArray(msg.tool_calls) && msg.tool_calls.length > 0;
}

function isSpecial(msg: ChatMessageView): boolean {
  // Cross-session messages carry a floating badge that must stay anchored -
  // keep them standalone so the badge lines up.
  return Boolean(msg.metadata?.cross_session);
}

/**
 * Groups consecutive same-role messages emitted within GROUP_WINDOW_MS.
 * Messages carrying tool_calls break the group so the reasoning trace
 * stays visually anchored to its own bubble with an avatar.
 */
export function groupMessages(messages: ChatMessageView[]): MessageGroup[] {
  const groups: MessageGroup[] = [];

  for (const msg of messages) {
    const last = groups.at(-1);
    const lastMsg = last?.messages.at(-1);

    const canGroup =
      last !== undefined &&
      lastMsg !== undefined &&
      GROUPABLE_ROLES.has(msg.role) &&
      last.role === msg.role &&
      !hasToolCalls(msg) &&
      !hasToolCalls(lastMsg) &&
      !isSpecial(msg) &&
      !isSpecial(lastMsg) &&
      new Date(msg.created_at).getTime() - new Date(lastMsg.created_at).getTime() < GROUP_WINDOW_MS;

    if (canGroup && last) {
      last.messages.push(msg);
    } else {
      groups.push({
        role: msg.role,
        key: msg.id,
        messages: [msg],
        startedAt: msg.created_at,
      });
    }
  }

  return groups;
}
