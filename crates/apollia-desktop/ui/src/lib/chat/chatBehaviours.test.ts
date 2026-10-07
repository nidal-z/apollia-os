import { describe, expect, test } from "vitest";
import {
  answerToRegenerate,
  doubleClickEdits,
  doubleClickRenames,
  emptyConversationToReopen,
  regenerateChord,
  type ListedConversation,
} from "./chatBehaviours";

function conversation(
  id: string,
  messageCount: number,
  extra: Partial<ListedConversation> = {},
): ListedConversation {
  return {
    id,
    status: "active",
    message_count: messageCount,
    created_at: "2026-10-01T09:00:00Z",
    ...extra,
  };
}

describe("one empty conversation at a time", () => {
  test("on, New conversation opens the empty conversation that exists", () => {
    // GIVEN a list holding one conversation with messages and one without
    const sessions = [conversation("busy", 6), conversation("empty", 0)];

    // WHEN New conversation asks which conversation to open again
    const reopened = emptyConversationToReopen(sessions, {}, true);

    // THEN it is the empty one: no second empty conversation is created
    expect(reopened).toBe("empty");
  });

  test("on, with no empty conversation, a new one is started", () => {
    // GIVEN a list where every conversation holds messages
    const sessions = [conversation("a", 2), conversation("b", 4)];

    // WHEN New conversation asks which conversation to open again
    // THEN none: the caller starts a new one
    expect(emptyConversationToReopen(sessions, {}, true)).toBeNull();
    expect(emptyConversationToReopen([], {}, true)).toBeNull();
  });

  test("off, an empty conversation is never opened again", () => {
    // GIVEN an empty conversation and the behaviour turned off
    const sessions = [conversation("empty", 0)];

    // WHEN New conversation asks which conversation to open again
    // THEN none: a second empty conversation may be created
    expect(emptyConversationToReopen(sessions, {}, false)).toBeNull();
  });

  test("a conversation that is closed, archived or just written in is not the empty one", () => {
    // GIVEN three conversations the list still counts no message for: one
    // closed, one archived, one a message was just sent in from this window
    const sessions = [
      conversation("closed", 0, { status: "closed" }),
      conversation("archived", 0, { archived: true }),
      conversation("just-sent", 0),
    ];
    const activity = { "just-sent": "2026-10-06T10:00:00Z" };

    // WHEN New conversation asks which conversation to open again
    // THEN none of them
    expect(emptyConversationToReopen(sessions, activity, true)).toBeNull();
  });

  test("of several empty conversations, the one created last opens", () => {
    // GIVEN two empty conversations left by an earlier setting
    const sessions = [
      conversation("first", 0, { created_at: "2026-10-01T09:00:00Z" }),
      conversation("last", 0, { created_at: "2026-10-05T09:00:00Z" }),
    ];

    // WHEN New conversation asks which conversation to open again
    // THEN the last created
    expect(emptyConversationToReopen(sessions, {}, true)).toBe("last");
  });
});

describe("the two double clicks", () => {
  const userMessage = { isUser: true, canEdit: true, busy: false, editing: false };

  test("double click to edit, off: a double click on a message opens nothing", () => {
    // GIVEN the default, off, and a message of the user that can be edited
    // WHEN it is double-clicked
    // THEN its edition does not open
    expect(doubleClickEdits({ ...userMessage, enabled: false })).toBe(false);
  });

  test("double click to edit, on: a double click on a message of the user opens its edition", () => {
    // GIVEN the behaviour on
    // WHEN a message of the user is double-clicked
    // THEN its edition opens, unless a turn is generating, the message is
    // already being edited or cannot be, or it is an answer
    expect(doubleClickEdits({ ...userMessage, enabled: true })).toBe(true);
    expect(doubleClickEdits({ ...userMessage, enabled: true, busy: true })).toBe(false);
    expect(doubleClickEdits({ ...userMessage, enabled: true, editing: true })).toBe(false);
    expect(doubleClickEdits({ ...userMessage, enabled: true, canEdit: false })).toBe(false);
    expect(doubleClickEdits({ ...userMessage, enabled: true, isUser: false })).toBe(false);
  });

  test("double click to rename, on: a double click on a name renames, as it always did", () => {
    // GIVEN the default, on, and a row that can be renamed
    // WHEN its name is double-clicked
    // THEN the renaming opens; a row that cannot be renamed stays as it is
    expect(doubleClickRenames(true, true)).toBe(true);
    expect(doubleClickRenames(true, false)).toBe(false);
  });

  test("double click to rename, off: a double click on a name renames nothing", () => {
    // GIVEN the behaviour off
    // WHEN the name of a row that can be renamed is double-clicked
    // THEN nothing opens
    expect(doubleClickRenames(false, true)).toBe(false);
  });
});

describe("Cmd R in the chat", () => {
  test("a model loaded and a conversation shown: the chord regenerates", () => {
    // GIVEN a model in memory, so the reload line does not show, the
    // behaviour on and a conversation shown
    // WHEN the chord is pressed
    const action = regenerateChord({ reloadShown: false, enabled: true, conversationOpen: true });

    // THEN it regenerates the last answer
    expect(action).toBe("regenerate");
  });

  test("no model loaded: the chord reloads the last model, whatever the setting", () => {
    // GIVEN no model in memory and one loaded before, so the reload line shows
    // WHEN the chord is pressed, the behaviour on then off
    const on = regenerateChord({ reloadShown: true, enabled: true, conversationOpen: true });
    const off = regenerateChord({ reloadShown: true, enabled: false, conversationOpen: true });

    // THEN it reloads the last model both times
    expect(on).toBe("reload");
    expect(off).toBe("reload");
  });

  test("the behaviour off, or no conversation shown: the chord is left alone", () => {
    // GIVEN a model in memory
    // WHEN the chord is pressed with the behaviour off, then with no
    // conversation on screen
    // THEN neither is handled
    expect(regenerateChord({ reloadShown: false, enabled: false, conversationOpen: true })).toBeNull();
    expect(regenerateChord({ reloadShown: false, enabled: true, conversationOpen: false })).toBeNull();
  });

  test("the answer regenerated is the last one of the thread", () => {
    // GIVEN a thread of two turns, the second with a tool message before its answer
    const thread = [
      { id: "u1", role: "user" },
      { id: "a1", role: "assistant" },
      { id: "u2", role: "user" },
      { id: "t2", role: "tool" },
      { id: "a2", role: "assistant" },
    ] as const;

    // WHEN the answer to regenerate is asked for
    // THEN it is the last answer; none while a turn is generating
    expect(answerToRegenerate(thread, false)).toBe("a2");
    expect(answerToRegenerate(thread, true)).toBeNull();
  });

  test("a thread without an answer to its last message has nothing to regenerate", () => {
    // GIVEN an empty thread, a thread ending on a message of the user, and a
    // greeting no message of the user precedes
    // WHEN the answer to regenerate is asked for
    // THEN there is none
    expect(answerToRegenerate([], false)).toBeNull();
    expect(
      answerToRegenerate(
        [
          { id: "u1", role: "user" },
          { id: "a1", role: "assistant" },
          { id: "u2", role: "user" },
        ],
        false,
      ),
    ).toBeNull();
    expect(answerToRegenerate([{ id: "hello", role: "assistant" }], false)).toBeNull();
  });
});
