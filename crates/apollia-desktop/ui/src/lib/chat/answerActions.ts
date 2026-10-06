/**
 * What the actions under a message do to the conversation: play a turn
 * again, continue the last answer, branch off, rewrite an answer, remove a
 * message.
 *
 * The conversation hands in what it owns (its thread, its turn state) through
 * `AnswerActionsHost`; the handlers hold no state of their own, which is why
 * they sit apart from the component and are tested without it.
 */
import {
  continueChatResponse,
  deleteChatMessage,
  editAndResendChatMessage,
  editChatResponse,
  forkChatSession,
  regenerateChatResponse,
  type ChatContinuation,
} from "$lib/ipc/chat";
import type { ChatMessageView } from "$lib/types";

/** What the conversation lends to the handlers. */
export interface AnswerActionsHost {
  sessionId(): string;
  messages(): ChatMessageView[];
  setMessages(next: ChatMessageView[]): void;
  /** True while a turn is generating: every action waits. */
  busy(): boolean;
  /** Put the conversation in the state of a turn that just started. */
  beginTurn(): Promise<void>;
  /** A turn that did not start: say why and reload the stored thread. */
  failTurn(error: unknown): void;
  /** Reload the stored thread after a change that played no turn. */
  reload(): Promise<void>;
  /** Open another conversation. */
  open(sessionId: string): void;
  /** Say that an action failed. */
  report(error: unknown): void;
}

export interface AnswerHandlers {
  regenerate(messageId: string): Promise<void>;
  resend(messageId: string, content: string): Promise<void>;
  continueAnswer(messageId: string): Promise<void>;
  fork(messageId: string): Promise<void>;
  editAnswer(messageId: string, content: string): Promise<void>;
  remove(messageId: string): Promise<void>;
}

/** What a message reads to draw its actions, set by the conversation. */
export interface AnswerActionsContext {
  /** The answer "continue" is offered on: the last message, when it is one. */
  readonly lastAnswerId: string | null;
  /**
   * Why the last answer cannot be continued, as an i18n key; `null` when it
   * can. The button is then disabled and shows this reason.
   */
  readonly continueRefusal: string | null;
  continueAnswer(messageId: string): void;
  fork(messageId: string): void;
  editAnswer(messageId: string, content: string): void;
  remove(messageId: string): void;
}

/** Context key the conversation sets and a message bubble reads. */
export const ANSWER_ACTIONS = Symbol("answer-actions");

/** The id of the last message when it is an answer with text, else `null`. */
export function lastAnswerId(messages: ChatMessageView[]): string | null {
  const last = messages[messages.length - 1];
  return last && last.role === "assistant" && last.content.trim() ? last.id : null;
}

/**
 * The reason "continue" is disabled, as an i18n key. A read that has not
 * answered, or failed, disables it too: the action is never offered on a
 * model nobody checked.
 */
export function continueRefusalKey(found: ChatContinuation | null): string | null {
  if (!found) return "chat.continue_unavailable_unknown";
  if (found.available) return null;
  return REFUSAL_KEYS[found.reason ?? "engine"];
}

/** Each reason the runtime gives, and the sentence shown for it. */
const REFUSAL_KEYS: Record<NonNullable<ChatContinuation["reason"]>, string> = {
  engine: "chat.continue_unavailable_engine",
  agent: "chat.continue_unavailable_agent",
  no_model: "chat.continue_unavailable_no_model",
};

/** True when the runtime marked the text of this message as rewritten. */
export function isEdited(message: ChatMessageView): boolean {
  return message.metadata?.edited === true;
}

export function createAnswerHandlers(host: AnswerActionsHost): AnswerHandlers {
  return {
    // Truncate the local thread back to the user turn, then let the runtime
    // replay it; the streamed answer renders as for a new message.
    async regenerate(messageId) {
      if (host.busy()) return;
      const msgs = host.messages();
      const idx = msgs.findIndex((m) => m.id === messageId);
      if (idx < 0) return;
      let userIdx = -1;
      for (let i = idx; i >= 0; i--) {
        if (msgs[i]!.role === "user") { userIdx = i; break; }
      }
      if (userIdx < 0) return;
      host.setMessages(msgs.slice(0, userIdx + 1));
      await host.beginTurn();
      try {
        await regenerateChatResponse(host.sessionId(), messageId);
      } catch (err: unknown) {
        host.failTurn(err);
      }
    },

    // Truncate the local thread before the edited message, show the new text
    // at once, then send it as a fresh turn.
    async resend(messageId, content) {
      if (host.busy()) return;
      const msgs = host.messages();
      const idx = msgs.findIndex((m) => m.id === messageId);
      if (idx < 0) return;
      const kept = msgs.slice(0, idx);
      host.setMessages([
        ...kept,
        {
          id: `temp-${Date.now()}`, role: "user", content,
          tool_calls: null, tool_name: null,
          seq: kept.length, created_at: new Date().toISOString(),
        },
      ]);
      await host.beginTurn();
      try {
        await editAndResendChatMessage(host.sessionId(), messageId, content);
      } catch (err: unknown) {
        host.failTurn(err);
      }
    },

    // The engine streams the answer again from its first word, followed by
    // the rest: the stored answer leaves the thread while the stream draws
    // it. The runtime keeps it until the turn lands, so a failure reloads it.
    async continueAnswer(messageId) {
      if (host.busy()) return;
      const msgs = host.messages();
      if (lastAnswerId(msgs) !== messageId) return;
      host.setMessages(msgs.slice(0, -1));
      await host.beginTurn();
      try {
        await continueChatResponse(host.sessionId(), messageId);
      } catch (err: unknown) {
        host.failTurn(err);
      }
    },

    async fork(messageId) {
      try {
        host.open(await forkChatSession(host.sessionId(), messageId));
      } catch (err: unknown) {
        host.report(err);
      }
    },

    async editAnswer(messageId, content) {
      if (host.busy()) return;
      try {
        await editChatResponse(host.sessionId(), messageId, content);
        await host.reload();
      } catch (err: unknown) {
        host.report(err);
      }
    },

    async remove(messageId) {
      if (host.busy()) return;
      try {
        await deleteChatMessage(host.sessionId(), messageId);
        await host.reload();
      } catch (err: unknown) {
        host.report(err);
      }
    },
  };
}
