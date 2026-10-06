/**
 * The actions under a message, as a conversation offers them to its bubbles.
 *
 * Whether the last answer can be continued depends on the model of the
 * conversation, so it is read again when the conversation or its model
 * changes; a read that arrives after the conversation was left is dropped.
 * The bubbles reach all of it through a context, several components below.
 */
import { setContext } from "svelte";
import {
  ANSWER_ACTIONS,
  continueRefusalKey,
  createAnswerHandlers,
  lastAnswerId,
  type AnswerActionsContext,
  type AnswerActionsHost,
  type AnswerHandlers,
} from "$lib/chat/answerActions";
import { getChatContinuation, type ChatContinuation } from "$lib/ipc/chat";

/** `model` is read inside the effect, so a change of model reads again. */
export function createAnswerActions(
  host: AnswerActionsHost,
  model: () => string | null | undefined,
): AnswerHandlers {
  const handlers = createAnswerHandlers(host);
  let continuation = $state<ChatContinuation | null>(null);

  $effect(() => {
    const id = host.sessionId();
    void model();
    let cancelled = false;
    continuation = null;
    getChatContinuation(id)
      .then((found) => {
        if (!cancelled) continuation = found;
      })
      .catch(() => {
        if (!cancelled) continuation = null;
      });
    return () => {
      cancelled = true;
    };
  });

  const context: AnswerActionsContext = {
    get lastAnswerId() {
      return lastAnswerId(host.messages());
    },
    get continueRefusal() {
      return continueRefusalKey(continuation);
    },
    continueAnswer: (messageId) => void handlers.continueAnswer(messageId),
    fork: (messageId) => void handlers.fork(messageId),
    editAnswer: (messageId, content) => void handlers.editAnswer(messageId, content),
    remove: (messageId) => void handlers.remove(messageId),
  };
  setContext(ANSWER_ACTIONS, context);
  return handlers;
}
