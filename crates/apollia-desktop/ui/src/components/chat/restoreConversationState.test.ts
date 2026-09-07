import { describe, test, expect } from "vitest";
import { get } from "svelte/store";
import { restoreConversationState } from "./restoreConversationState";
import {
  pendingUserInputs,
  addPendingUserInput,
  getPendingUserInputForSession,
} from "$lib/stores/chat-global";

/**
 * The conversation view is reused from one session to the next, so what it
 * shows has to be rebuilt from the session it now displays. These tests cover
 * the two rules that let a question asked in one conversation appear in
 * another, and the answer be attributed to a session that never asked.
 */

const QUESTION = {
  request_id: "req-1",
  questions_json: JSON.stringify([
    { id: "q1", question: "Your name?", question_type: "open", options: [] },
  ]),
  context: null,
};

function inputFor(
  sessionId: string,
  userInput: typeof QUESTION | null,
  hasUserInput: boolean,
) {
  return {
    sessionId,
    buffers: {},
    sessionStatus: "active" as const,
    approval: null,
    userInput,
    hasUserInput,
  };
}

describe("a question shown by the conversation belongs to the session shown", () => {
  test("restores the question the session carries", () => {
    // GIVEN a session whose pending question the store knows, on a view that
    // shows none
    const input = inputFor("session-a", QUESTION, false);

    // WHEN the view rebuilds its state for that session
    const patch = restoreConversationState(input);

    // THEN the question is restored
    expect(patch.userInput?.requestId).toBe("req-1");
  });

  test("clears a question the shown session does not have", () => {
    // GIVEN a view still showing the previous session's question, on a
    // session the store has no question for
    const input = inputFor("session-b", null, true);

    // WHEN the view rebuilds its state for that session
    const patch = restoreConversationState(input);

    // THEN the stale question is cleared rather than left asking here
    expect(patch.userInput).toBeNull();
  });

  test("leaves the shown question alone when it is the session's own", () => {
    // GIVEN a view already showing the question of the session it displays
    const input = inputFor("session-a", QUESTION, true);

    // WHEN the view rebuilds its state
    const patch = restoreConversationState(input);

    // THEN nothing is patched, so the card keeps its own state
    expect(patch.userInput).toBeUndefined();
  });
});

describe("the store hands a question to the session that asked it", () => {
  test("an unattributed question reaches no conversation", () => {
    // GIVEN a pending question carrying no session id, as an older runtime
    // could produce
    pendingUserInputs.set([]);
    addPendingUserInput({ ...QUESTION, session_id: "", created_at: "2026-09-07T00:00:00Z" });

    // WHEN any conversation asks the store for its question
    const found = getPendingUserInputForSession("session-a");

    // THEN it receives none: an unattributed question is answered from the
    // inbox, never in a conversation that did not ask
    expect(found).toBeNull();
    expect(get(pendingUserInputs)).toHaveLength(1);
    pendingUserInputs.set([]);
  });

  test("a stamped question reaches its own conversation", () => {
    // GIVEN a pending question stamped with the session that asked
    pendingUserInputs.set([]);
    addPendingUserInput({
      ...QUESTION,
      session_id: "session-a",
      created_at: "2026-09-07T00:00:00Z",
    });

    // WHEN that conversation asks the store
    const found = getPendingUserInputForSession("session-a");

    // THEN it receives the question, and its neighbour does not
    expect(found?.request_id).toBe("req-1");
    expect(getPendingUserInputForSession("session-b")).toBeNull();
    pendingUserInputs.set([]);
  });
});
