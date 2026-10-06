import { describe, test, expect, vi, beforeEach } from "vitest";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

import {
  continueRefusalKey,
  createAnswerHandlers,
  isEdited,
  lastAnswerId,
  type AnswerActionsHost,
} from "./answerActions";
import type { ChatMessageView } from "$lib/types";

function message(id: string, role: ChatMessageView["role"], content = id): ChatMessageView {
  return { id, role, content, tool_calls: null, tool_name: null, seq: 0, created_at: "" };
}

/** A conversation of two turns, and what the handlers did to it. */
function conversation(busy = false) {
  const state = {
    messages: [
      message("u-1", "user"),
      message("a-1", "assistant"),
      message("u-2", "user"),
      message("a-2", "assistant"),
    ],
    turnsBegun: 0,
    turnsFailed: [] as unknown[],
    reloads: 0,
    opened: [] as string[],
    reported: [] as unknown[],
  };
  const host: AnswerActionsHost = {
    sessionId: () => "s-1",
    messages: () => state.messages,
    setMessages: (next) => void (state.messages = next),
    busy: () => busy,
    beginTurn: async () => void state.turnsBegun++,
    failTurn: (err) => void state.turnsFailed.push(err),
    reload: async () => void state.reloads++,
    open: (id) => void state.opened.push(id),
    report: (err) => void state.reported.push(err),
  };
  return { state, handlers: createAnswerHandlers(host) };
}

const ids = (messages: ChatMessageView[]) => messages.map((m) => m.id);

beforeEach(() => {
  invoke.mockReset();
  invoke.mockResolvedValue(undefined);
});

describe("the actions under a message", () => {
  test("branching off asks for the messages up to the answer and opens the new conversation", async () => {
    // GIVEN a conversation, and a runtime that answers the id of the branch
    const { state, handlers } = conversation();
    invoke.mockResolvedValue("s-2");

    // WHEN the first answer is branched off
    await handlers.fork("a-1");

    // THEN the command names the conversation and the answer, the new
    // conversation opens, and this one is untouched
    expect(invoke).toHaveBeenCalledWith("fork_chat_session", { sessionId: "s-1", messageId: "a-1" });
    expect(state.opened).toEqual(["s-2"]);
    expect(ids(state.messages)).toEqual(["u-1", "a-1", "u-2", "a-2"]);
  });

  test("continuing takes the last answer out of the thread while its turn runs again", async () => {
    // GIVEN a conversation at rest
    const { state, handlers } = conversation();

    // WHEN its last answer is continued
    await handlers.continueAnswer("a-2");

    // THEN the stored answer leaves the thread for the stream to draw it, a
    // turn begins, and the runtime is asked to continue that answer
    expect(ids(state.messages)).toEqual(["u-1", "a-1", "u-2"]);
    expect(state.turnsBegun).toBe(1);
    expect(invoke).toHaveBeenCalledWith("continue_chat_response", { sessionId: "s-1", messageId: "a-2" });
  });

  test("an answer that is not the last, or a busy conversation, is not continued", async () => {
    // GIVEN a conversation at rest, and one in the middle of a turn
    const idle = conversation();
    const busy = conversation(true);

    // WHEN an earlier answer is continued, and the last one of the busy one
    await idle.handlers.continueAnswer("a-1");
    await busy.handlers.continueAnswer("a-2");

    // THEN nothing was asked and both threads are whole
    expect(invoke).not.toHaveBeenCalled();
    expect(idle.state.messages).toHaveLength(4);
    expect(busy.state.messages).toHaveLength(4);
  });

  test("a continuation the runtime refuses is reported as a turn that did not start", async () => {
    // GIVEN a runtime that refuses to continue
    const { state, handlers } = conversation();
    invoke.mockRejectedValue("the answer cannot be continued: engine");

    // WHEN the last answer is continued
    await handlers.continueAnswer("a-2");

    // THEN the refusal reaches the conversation, which reloads its thread
    expect(state.turnsFailed).toEqual(["the answer cannot be continued: engine"]);
  });

  test("deleting and rewriting change the stored thread and reload it, without a turn", async () => {
    // GIVEN a conversation at rest
    const { state, handlers } = conversation();

    // WHEN a message is deleted, then an answer rewritten
    await handlers.remove("u-2");
    await handlers.editAnswer("a-1", "Rewritten.");

    // THEN each reached its command, the thread was reloaded twice and no
    // turn began
    expect(invoke).toHaveBeenNthCalledWith(1, "delete_chat_message", { sessionId: "s-1", messageId: "u-2" });
    expect(invoke).toHaveBeenNthCalledWith(2, "edit_chat_response", {
      sessionId: "s-1",
      messageId: "a-1",
      content: "Rewritten.",
    });
    expect(state.reloads).toBe(2);
    expect(state.turnsBegun).toBe(0);
  });

  test("a deletion that fails is reported and reloads nothing", async () => {
    // GIVEN a runtime that refuses the deletion
    const { state, handlers } = conversation();
    invoke.mockRejectedValue("message not found: a-1");

    // WHEN an answer is deleted
    await handlers.remove("a-1");

    // THEN the refusal is shown and the thread is left as it is
    expect(state.reported).toEqual(["message not found: a-1"]);
    expect(state.reloads).toBe(0);
  });

  test("regenerating and resending keep truncating the thread before the turn runs again", async () => {
    // GIVEN two conversations at rest
    const regenerated = conversation();
    const resent = conversation();

    // WHEN the last answer of one is regenerated, and the second user
    // message of the other is edited
    await regenerated.handlers.regenerate("a-2");
    await resent.handlers.resend("u-2", "Again?");

    // THEN the first ends on its user message, the second on the new text,
    // and each asked its command
    expect(ids(regenerated.state.messages)).toEqual(["u-1", "a-1", "u-2"]);
    expect(resent.state.messages.map((m) => m.content)).toEqual(["u-1", "a-1", "Again?"]);
    expect(invoke).toHaveBeenCalledWith("regenerate_chat_response", { sessionId: "s-1", messageId: "a-2" });
    expect(invoke).toHaveBeenCalledWith("edit_and_resend_chat_message", {
      sessionId: "s-1",
      messageId: "u-2",
      content: "Again?",
    });
  });
});

describe("what a message reads to draw its actions", () => {
  test("the last answer is the last message, when it is an answer with text", () => {
    // GIVEN threads ending on an answer, on a user message, on an empty answer
    const answered = [message("u-1", "user"), message("a-1", "assistant")];
    const asked = [...answered, message("u-2", "user")];
    const empty = [...asked, message("a-2", "assistant", "  ")];

    // WHEN the last answer of each is read
    // THEN only the first has one
    expect(lastAnswerId(answered)).toBe("a-1");
    expect(lastAnswerId(asked)).toBeNull();
    expect(lastAnswerId(empty)).toBeNull();
    expect(lastAnswerId([])).toBeNull();
  });

  test("continue is offered only on a model that was read to continue an answer", () => {
    // GIVEN what the runtime can answer about the model, and no answer yet
    // WHEN the reason is read
    // THEN only an available continuation has none
    expect(continueRefusalKey({ available: true, reason: null })).toBeNull();
    expect(continueRefusalKey({ available: false, reason: "engine" })).toBe("chat.continue_unavailable_engine");
    expect(continueRefusalKey({ available: false, reason: "agent" })).toBe("chat.continue_unavailable_agent");
    expect(continueRefusalKey({ available: false, reason: "no_model" })).toBe("chat.continue_unavailable_no_model");
    expect(continueRefusalKey(null)).toBe("chat.continue_unavailable_unknown");
  });

  test("a message is edited when the runtime marked it so", () => {
    // GIVEN an answer the runtime marked, and one it did not
    const marked = { ...message("a-1", "assistant"), metadata: { edited: true } };

    // WHEN each is read
    // THEN only the marked one is edited
    expect(isEdited(marked)).toBe(true);
    expect(isEdited(message("a-2", "assistant"))).toBe(false);
  });
});
