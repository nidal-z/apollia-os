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

const SESSIONS: DecoratedSession[] = [
  session("b", "Budget", "2026-10-02T09:00:00Z", "2026-10-02T09:00:00Z"),
  session("a", "achats", "2026-10-01T09:00:00Z", "2026-10-04T09:00:00Z"),
  session("c", "Courrier", "2026-10-03T09:00:00Z", "2026-10-03T10:00:00Z"),
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

    // THEN creation ascending, latest activity first, and the name A to Z,
    // case aside, each give their own order
    expect(orders).toEqual({
      newest: ["c", "b", "a"],
      oldest: ["a", "b", "c"],
      activity: ["a", "c", "b"],
      name: ["a", "b", "c"],
    });
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
