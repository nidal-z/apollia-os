/**
 * What the behaviours of Settings > Chat decide, as functions of the state
 * they read: which empty conversation "New conversation" opens again, whether
 * a double click renames or edits, and what the regeneration shortcut does.
 */
import type { ChatMessageView } from "$lib/types";

/** What the rule reads of a conversation of the list. */
export interface ListedConversation {
  id: string;
  status: string;
  message_count: number;
  created_at: string;
  archived?: boolean;
}

/**
 * The empty conversation "New conversation" opens again instead of starting
 * another one, `null` when the rule is off or none is empty.
 *
 * A conversation is empty when it holds no message and none was sent from
 * this window since the list was read (`activity`, by session id). A closed
 * or archived conversation is never opened again. Of several, the one
 * created last.
 */
export function emptyConversationToReopen(
  sessions: readonly ListedConversation[],
  activity: Readonly<Record<string, string>>,
  singleEmpty: boolean,
): string | null {
  if (!singleEmpty) return null;
  const empty = sessions.filter(
    (s) =>
      s.status !== "closed" && !s.archived && s.message_count === 0 && !(s.id in activity),
  );
  if (empty.length === 0) return null;
  return empty.reduce((last, s) => (s.created_at > last.created_at ? s : last)).id;
}

/**
 * Whether a double click on the name of a conversation or of a folder opens
 * its renaming: the behaviour is on and the row can be renamed.
 */
export function doubleClickRenames(enabled: boolean, canRename: boolean): boolean {
  return enabled && canRename;
}

/**
 * Whether a double click on a message of the user opens its edition: the
 * behaviour is on, the message can be edited, no turn is generating and it is
 * not being edited already.
 */
export function doubleClickEdits(input: {
  enabled: boolean;
  isUser: boolean;
  canEdit: boolean;
  busy: boolean;
  editing: boolean;
}): boolean {
  return input.enabled && input.isUser && input.canEdit && !input.busy && !input.editing;
}

/** What Cmd R (Ctrl R outside the Mac) does in the chat. */
export type RegenerateChord = "reload" | "regenerate" | null;

/**
 * One chord, two meanings by state. With no model in memory and one loaded
 * before, it loads that model again, whatever the setting. Otherwise, with
 * the behaviour on and a conversation shown, it regenerates the last answer.
 * In any other state the chord is left alone.
 */
export function regenerateChord(input: {
  /** The "Reload the last model" line shows. */
  reloadShown: boolean;
  /** The behaviour of Settings > Chat. */
  enabled: boolean;
  /** A conversation is shown. */
  conversationOpen: boolean;
}): RegenerateChord {
  if (input.reloadShown) return "reload";
  return input.enabled && input.conversationOpen ? "regenerate" : null;
}

/**
 * The answer the shortcut regenerates: the last one of the thread, when it
 * follows a message of the user and no turn is generating; `null` otherwise.
 */
export function answerToRegenerate(
  messages: readonly Pick<ChatMessageView, "id" | "role">[],
  busy: boolean,
): string | null {
  if (busy) return null;
  for (let i = messages.length - 1; i >= 0; i--) {
    const message = messages[i]!;
    if (message.role === "user") return null;
    if (message.role === "assistant") {
      return messages.slice(0, i).some((m) => m.role === "user") ? message.id : null;
    }
  }
  return null;
}

/**
 * The answer a request regenerates: the one `answerToRegenerate` names, and
 * only when it is `answerId`, if the request names one. A request made from
 * an answer another turn has followed regenerates nothing.
 */
export function answerRequested(
  messages: readonly Pick<ChatMessageView, "id" | "role">[],
  busy: boolean,
  answerId: string | undefined,
): string | null {
  const id = answerToRegenerate(messages, busy);
  if (answerId !== undefined && id !== answerId) return null;
  return id;
}
