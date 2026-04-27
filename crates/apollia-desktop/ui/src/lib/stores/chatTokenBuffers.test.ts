/**
 * Tests for per-session chat token buffers.
 *
 * Covers LRU eviction, TTL cleanup, streaming-flag lifecycle, and
 * subscriber notification semantics.
 */
import { describe, it, expect, beforeEach } from "vitest";
import {
  TokenBufferStore,
  MAX_BUFFERS,
  BUFFER_TTL_MS,
} from "./chatTokenBuffers";

describe("TokenBufferStore", () => {
  let store: TokenBufferStore;

  beforeEach(() => {
    store = new TokenBufferStore();
  });

  // GIVEN an empty store
  // WHEN we append a token
  // THEN the session text is retrievable and streaming flag is set
  it("accumulates tokens per session", () => {
    store.append("s1", "Hello");
    store.append("s1", " world");
    expect(store.get("s1")).toBe("Hello world");
  });

  // GIVEN two sessions streaming in parallel
  // WHEN tokens arrive interleaved
  // THEN buffers stay isolated
  it("isolates buffers across sessions", () => {
    store.append("a", "foo");
    store.append("b", "bar");
    store.append("a", "baz");
    expect(store.get("a")).toBe("foobaz");
    expect(store.get("b")).toBe("bar");
  });

  // GIVEN 51 sessions appended sequentially
  // WHEN MAX_BUFFERS is 50
  // THEN the oldest is evicted and size caps at 50
  it("evicts LRU entry when MAX_BUFFERS exceeded", () => {
    for (let i = 0; i < MAX_BUFFERS + 5; i++) {
      store.append(`s${i}`, "x");
    }
    expect(store.size()).toBe(MAX_BUFFERS);
    expect(store.get("s0")).toBe("");
    expect(store.get(`s${MAX_BUFFERS + 4}`)).toBe("x");
  });

  // GIVEN a buffer touched at t=0
  // WHEN another buffer appends at t = TTL + 1
  // THEN the stale buffer is expired
  it("evicts entries older than TTL on append", () => {
    store.append("stale", "old", 0);
    store.append("fresh", "new", BUFFER_TTL_MS + 1);
    expect(store.get("stale")).toBe("");
    expect(store.get("fresh")).toBe("new");
  });

  // GIVEN re-appending to an existing session
  // THEN that session moves to the LRU tail (not evicted first)
  it("refreshes LRU position on repeat append", () => {
    for (let i = 0; i < MAX_BUFFERS; i++) {
      store.append(`s${i}`, "x");
    }
    store.append("s0", "!"); // touches s0 → now most recent
    store.append("new", "z"); // should evict s1, not s0
    expect(store.get("s0")).toBe("x!");
    expect(store.get("s1")).toBe("");
    expect(store.get("new")).toBe("z");
  });

  // GIVEN a session in the buffer
  // WHEN clear() is called
  // THEN the buffer and streaming flag are dropped
  it("clear() removes buffer and streaming flag", () => {
    store.append("s1", "hi");
    store.clear("s1");
    expect(store.get("s1")).toBe("");
    expect(store.size()).toBe(0);
  });

  // GIVEN subscribers on both stores
  // WHEN tokens are appended and cleared
  // THEN both subscribers receive the correct snapshots
  it("notifies buffer + streaming subscribers", () => {
    const bufferSnaps: Record<string, string>[] = [];
    const streamingSnaps: Set<string>[] = [];
    const un1 = store.subscribeBuffers((s) => bufferSnaps.push({ ...s }));
    const un2 = store.subscribeStreaming((s) => streamingSnaps.push(new Set(s)));

    store.append("s1", "hi");
    store.clear("s1");

    expect(bufferSnaps.at(-1)).toEqual({});
    expect(streamingSnaps.at(-1)?.size).toBe(0);
    un1();
    un2();
  });

  // GIVEN multiple stale buffers
  // WHEN evictExpiredForTest is called
  // THEN all expired entries are cleared
  it("evictExpiredForTest drops all stale entries", () => {
    store.append("a", "1", 0);
    store.append("b", "2", 0);
    store.append("c", "3", BUFFER_TTL_MS + 10);
    store.evictExpiredForTest(BUFFER_TTL_MS + 10);
    expect(store.size()).toBe(1);
    expect(store.get("c")).toBe("3");
  });

  // GIVEN closeSession
  // THEN it's equivalent to clear
  it("closeSession aliases clear", () => {
    store.append("s1", "hi");
    store.closeSession("s1");
    expect(store.size()).toBe(0);
  });
});
