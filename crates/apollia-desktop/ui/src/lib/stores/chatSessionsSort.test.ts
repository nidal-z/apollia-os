import { describe, test, expect } from "vitest";
import {
  SESSION_SORTS,
  lastActivity,
  sidebarSessions,
  type DecoratedSession,
  type SessionSort,
} from "./chatSessions";

// The sort of the conversation list used to be persisted and read by nobody:
// the Chat list filtered the sessions and kept the order they came in. These
// tests hold the list the Chat screen shows to the order chosen in its menu.

function session(
  id: string,
  title: string,
  createdAt: string,
  activityAt: string = createdAt,
  extra: Partial<DecoratedSession> = {},
): DecoratedSession {
  return {
    id,
    mode: "libre",
    agent_name: null,
    status: "active",
    last_message_preview: null,
    message_count: 0,
    created_at: createdAt,
    closed_at: null,
    title,
    pinned: false,
    archived: false,
    unread: false,
    lastActivityAt: activityAt,
    ...extra,
  };
}

// "b" holds the most messages and the fewest generated tokens: an order by
// message count and an order by tokens cannot be mistaken for one another.
const SESSIONS: DecoratedSession[] = [
  session("b", "Budget", "2026-10-02T09:00:00Z", "2026-10-02T09:00:00Z", {
    message_count: 40,
    output_tokens: 300,
  }),
  session("a", "achats", "2026-10-01T09:00:00Z", "2026-10-04T09:00:00Z", {
    message_count: 2,
    output_tokens: 9000,
  }),
  session("c", "Courrier", "2026-10-03T09:00:00Z", "2026-10-03T10:00:00Z", {
    message_count: 12,
    output_tokens: 1200,
  }),
];

function order(sort: SessionSort, query = ""): string[] {
  return sidebarSessions(SESSIONS, query, sort).map((s) => s.id);
}

describe("chatSessions - the order of the Chat list", () => {
  test("by default the newest conversation comes first", () => {
    // GIVEN the first choice of the menu, the default
    const sort = SESSION_SORTS[0];

    // WHEN the list is ordered
    const ids = order(sort);

    // THEN it is the newest first, by creation
    expect(sort).toBe("newest");
    expect(ids).toEqual(["c", "b", "a"]);
  });

  test("each choice of the menu orders the list its own way", () => {
    // GIVEN three conversations created, used and named differently
    // WHEN the list is ordered by each choice
    const orders = Object.fromEntries(SESSION_SORTS.map((s) => [s, order(s)]));

    // THEN creation ascending, latest activity first, the name A to Z, case
    // aside, and the generated tokens each give their own order
    expect(orders).toEqual({
      newest: ["c", "b", "a"],
      oldest: ["a", "b", "c"],
      activity: ["a", "c", "b"],
      name: ["a", "b", "c"],
      tokens: ["a", "c", "b"],
    });
  });

  test("the token order reads the generated tokens, never the message count", () => {
    // GIVEN the conversation with the most messages holding the fewest tokens
    // WHEN the list is ordered by tokens
    const ids = order("tokens");

    // THEN the most tokens come first: by message count it would be b, c, a
    expect(ids).toEqual(["a", "c", "b"]);
    expect(ids).not.toEqual(["b", "c", "a"]);
  });

  test("conversations of the same token count keep the newest first, and no count is zero", () => {
    // GIVEN two conversations the runtime gave no token count for, and one
    // that holds tokens
    const sessions = [
      session("old", "Old", "2026-10-01T09:00:00Z"),
      session("heavy", "Heavy", "2026-09-01T09:00:00Z", undefined, { output_tokens: 50 }),
      session("new", "New", "2026-10-05T09:00:00Z"),
    ];

    // WHEN the list is ordered by tokens
    const ids = sidebarSessions(sessions, "", "tokens").map((s) => s.id);

    // THEN the one with tokens leads, and the two without follow newest first
    expect(ids).toEqual(["heavy", "new", "old"]);
  });

  test("the search and the archive still apply before the order", () => {
    // GIVEN an archived conversation, and a search on a title
    const withArchived = [
      ...SESSIONS,
      session("z", "Archived budget", "2026-10-05T09:00:00Z", undefined, { archived: true }),
    ];

    // WHEN the list is filtered and ordered
    const ids = sidebarSessions(withArchived, "bud", "oldest").map((s) => s.id);

    // THEN only the matching conversation that is not archived remains
    expect(ids).toEqual(["b"]);
  });

  test("the activity of a conversation is its last message sent, else its creation", () => {
    // GIVEN a conversation created on the first, with a message sent on the third
    const created = session("x", "X", "2026-10-01T09:00:00Z");

    // WHEN its activity is read with and without the noted message
    const noted = lastActivity(created, "2026-10-03T09:00:00Z");
    const bare = lastActivity(created);

    // THEN the message wins, and creation stands in without it
    expect(noted).toBe("2026-10-03T09:00:00Z");
    expect(bare).toBe("2026-10-01T09:00:00Z");
  });
});
