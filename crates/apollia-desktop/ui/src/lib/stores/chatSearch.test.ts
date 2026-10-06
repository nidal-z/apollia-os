import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import { get } from "svelte/store";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import {
  MESSAGE_SEARCH_DELAY_MS,
  NO_MESSAGE_SEARCH,
  clearMessageSearch,
  createMessageSearcher,
  hitsFor,
  messageSearch,
  searchMessagesFor,
  searchSections,
  searchesMessages,
  type MessageSearchState,
} from "./chatSearch";
import type { MessageSearchHit } from "$lib/ipc/chat";

const invoke = vi.mocked(tauriInvoke);

function hit(sessionId: string, excerpt: string): MessageSearchHit {
  return {
    session_id: sessionId,
    title: null,
    agent_name: null,
    created_at: "2026-10-01T00:00:00Z",
    excerpt,
  };
}

/** A promise settled by hand, to order the answers of two searches. */
function deferred<T>(): { promise: Promise<T>; resolve: (value: T) => void } {
  let resolve: (value: T) => void = () => {};
  const promise = new Promise<T>((r) => (resolve = r));
  return { promise, resolve };
}

beforeEach(() => {
  vi.useFakeTimers();
  invoke.mockReset();
  clearMessageSearch();
});

afterEach(() => {
  vi.useRealTimers();
});

describe("chat search - what a search shows", () => {
  test("title matches come first, message matches follow with their excerpt", () => {
    // GIVEN a list of three conversations, one kept by the title filter, and
    // two found by the runtime in their messages
    const listed = [{ id: "s-title" }, { id: "s-march" }, { id: "s-close" }];
    const hits = [
      hit("s-march", "... the highest is the one of Durand ..."),
      hit("s-close", "... deposit of Durand received ..."),
    ];

    // WHEN the answer is laid out
    const sections = searchSections([{ id: "s-title" }], hits, listed);

    // THEN the two sections are those of the mockup, each message match
    // under its passage
    expect(sections.titles).toEqual([{ id: "s-title" }]);
    expect(sections.messages).toEqual([
      { session: { id: "s-march" }, excerpt: "... the highest is the one of Durand ..." },
      { session: { id: "s-close" }, excerpt: "... deposit of Durand received ..." },
    ]);
  });

  test("a conversation found both ways is shown once, and one the list hides is dropped", () => {
    // GIVEN a conversation matched by title and by message, and a hit on a
    // conversation the list does not show
    const listed = [{ id: "s-both" }];
    const hits = [hit("s-both", "... Durand ..."), hit("s-archived", "... Durand ...")];

    // WHEN the answer is laid out
    const sections = searchSections([{ id: "s-both" }], hits, listed);

    // THEN it is under the titles only, and the hidden one nowhere
    expect(sections.titles).toEqual([{ id: "s-both" }]);
    expect(sections.messages).toEqual([]);
  });

  test("the hits of an older query are not shown under a newer one", () => {
    // GIVEN the answer of the search "dur"
    const state: MessageSearchState = { query: "dur", hits: [hit("s-1", "Durand")], failed: false };

    // WHEN the hits are asked for that query, typed with spaces, and for a longer one
    // THEN only the query answered shows them
    expect(hitsFor(state, " dur ")).toHaveLength(1);
    expect(hitsFor(state, "durand")).toEqual([]);
  });
});

describe("chat search - asking the runtime", () => {
  test("one letter filters titles only; two letters search the messages", () => {
    // GIVEN queries of zero, one and two letters
    // WHEN each is judged
    // THEN the runtime is asked from two letters on
    expect(searchesMessages("")).toBe(false);
    expect(searchesMessages(" d ")).toBe(false);
    expect(searchesMessages("du")).toBe(true);
  });

  test("the runtime is asked once typing pauses, with the trimmed words", async () => {
    // GIVEN a runtime that finds one conversation
    invoke.mockResolvedValue([hit("s-1", "... Durand ...")]);
    const states: MessageSearchState[] = [];
    const searcher = createMessageSearcher((state) => states.push(state));

    // WHEN three keystrokes follow each other faster than the pause
    searcher.search("d");
    searcher.search("du");
    searcher.search(" dur ");
    const before = invoke.mock.calls.length;
    await vi.advanceTimersByTimeAsync(MESSAGE_SEARCH_DELAY_MS);

    // THEN nothing was asked while typing, then one search with the last
    // words, and its answer is given
    expect(before).toBe(0);
    expect(invoke).toHaveBeenCalledTimes(1);
    expect(invoke).toHaveBeenCalledWith("search_chat_messages", { query: "dur", limit: undefined });
    expect(states.at(-1)).toEqual({
      query: "dur",
      hits: [hit("s-1", "... Durand ...")],
      failed: false,
    });
  });

  test("an answer that arrives after a newer search is dropped", async () => {
    // GIVEN a slow search for "dur" and a fast one for "durand"
    const slow = deferred<MessageSearchHit[]>();
    const fast = deferred<MessageSearchHit[]>();
    invoke.mockReturnValueOnce(slow.promise).mockReturnValueOnce(fast.promise);
    const states: MessageSearchState[] = [];
    const searcher = createMessageSearcher((state) => states.push(state));

    // WHEN both are asked, and the first answers last
    searcher.search("dur");
    await vi.advanceTimersByTimeAsync(MESSAGE_SEARCH_DELAY_MS);
    searcher.search("durand");
    await vi.advanceTimersByTimeAsync(MESSAGE_SEARCH_DELAY_MS);
    fast.resolve([hit("s-new", "Durand")]);
    await vi.advanceTimersByTimeAsync(0);
    slow.resolve([hit("s-old", "durable")]);
    await vi.advanceTimersByTimeAsync(0);

    // THEN only the answer of the last search was given
    expect(states).toEqual([{ query: "durand", hits: [hit("s-new", "Durand")], failed: false }]);
  });

  test("emptying the field forgets the search without asking", async () => {
    // GIVEN a search about to be asked
    invoke.mockResolvedValue([hit("s-1", "Durand")]);
    searchMessagesFor("dur");

    // WHEN the field is emptied before the pause ends
    searchMessagesFor("");
    await vi.advanceTimersByTimeAsync(MESSAGE_SEARCH_DELAY_MS * 2);

    // THEN the runtime was never asked and the list shows no hit
    expect(invoke).not.toHaveBeenCalled();
    expect(get(messageSearch)).toEqual(NO_MESSAGE_SEARCH);
  });

  test("a search the runtime refuses is said to have failed, with no hit", async () => {
    // GIVEN a runtime that refuses the search
    invoke.mockRejectedValue(new Error("chat subsystem not available"));
    const quiet = vi.spyOn(console, "warn").mockImplementation(() => {});

    // WHEN the list searches
    searchMessagesFor("dur");
    await vi.advanceTimersByTimeAsync(MESSAGE_SEARCH_DELAY_MS);
    quiet.mockRestore();

    // THEN the list holds a failed search for that query
    expect(get(messageSearch)).toEqual({ query: "dur", hits: [], failed: true });
  });
});
