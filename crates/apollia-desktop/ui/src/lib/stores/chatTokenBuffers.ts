/**
 * Per-session chat token buffers with LRU eviction and TTL cleanup.
 *
 * Rationale:
 *  - A single global buffer caused token leaks across sessions and re-renders
 *    on every append of every conversation.
 *  - A per-session Map avoids cross-talk.  LRU caps memory (max 50 active
 *    buffers) and a 30-min TTL drops abandoned buffers automatically.
 *  - Buffers are cleared on session close, on response completion, and on
 *    explicit error paths.
 */
import type { Readable } from "svelte/store";

/** Maximum concurrent session buffers before LRU eviction kicks in. */
export const MAX_BUFFERS = 50;

/** Time-to-live for an inactive buffer, in milliseconds (30 min). */
export const BUFFER_TTL_MS = 30 * 60_000;

interface Entry {
  text: string;
  lastAccess: number;
}

type BuffersSnapshot = Record<string, string>;
type StreamingSnapshot = Set<string>;

/**
 * LRU+TTL token buffer store.  A single instance backs the Svelte-facing
 * `globalTokenBuffers` / `globalStreamingSessions` readable stores plus the
 * imperative `append/get/clear/closeSession` helpers.
 */
export class TokenBufferStore {
  private readonly map = new Map<string, Entry>();
  private readonly streaming = new Set<string>();
  private readonly buffersSubs = new Set<(s: BuffersSnapshot) => void>();
  private readonly streamingSubs = new Set<(s: StreamingSnapshot) => void>();

  subscribeBuffers(fn: (s: BuffersSnapshot) => void): () => void {
    fn(this.snapshotBuffers());
    this.buffersSubs.add(fn);
    return () => {
      this.buffersSubs.delete(fn);
    };
  }

  subscribeStreaming(fn: (s: StreamingSnapshot) => void): () => void {
    fn(new Set(this.streaming));
    this.streamingSubs.add(fn);
    return () => {
      this.streamingSubs.delete(fn);
    };
  }

  /** Append a streamed token for `sessionId`.  Refreshes LRU + TTL. */
  append(sessionId: string, token: string, now: number = Date.now()): void {
    const expiredChanged = this.evictExpired(now);

    const existing = this.map.get(sessionId);
    if (existing === undefined) {
      this.map.set(sessionId, { text: token, lastAccess: now });
      while (this.map.size > MAX_BUFFERS) {
        const oldestKey = this.map.keys().next().value;
        if (oldestKey === undefined) break;
        this.map.delete(oldestKey);
        this.streaming.delete(oldestKey);
      }
    } else {
      // Re-insert to move to tail of insertion order (LRU = head).
      this.map.delete(sessionId);
      this.map.set(sessionId, {
        text: existing.text + token,
        lastAccess: now,
      });
    }

    const streamingChanged = !this.streaming.has(sessionId);
    if (streamingChanged) this.streaming.add(sessionId);

    this.notifyBuffers();
    if (expiredChanged || streamingChanged) this.notifyStreaming();
  }

  /** Current text for a session (empty string if unknown). */
  get(sessionId: string, now: number = Date.now()): string {
    const entry = this.map.get(sessionId);
    if (entry === undefined) return "";
    entry.lastAccess = now;
    return entry.text;
  }

  /** Drop a session's buffer and remove the streaming flag. */
  clear(sessionId: string): void {
    const hadBuffer = this.map.delete(sessionId);
    const hadStreaming = this.streaming.delete(sessionId);
    if (hadBuffer) this.notifyBuffers();
    if (hadStreaming) this.notifyStreaming();
  }

  /** Alias called explicitly when a session is closed by the user / runtime. */
  closeSession(sessionId: string): void {
    this.clear(sessionId);
  }

  /** Current size - exposed for tests. */
  size(): number {
    return this.map.size;
  }

  /** Force eviction pass - exposed for tests. */
  evictExpiredForTest(now: number = Date.now()): void {
    if (this.evictExpired(now)) {
      this.notifyBuffers();
      this.notifyStreaming();
    }
  }

  /** Drop everything.  Intended for tests. */
  reset(): void {
    const hadBuffers = this.map.size > 0;
    const hadStreaming = this.streaming.size > 0;
    this.map.clear();
    this.streaming.clear();
    if (hadBuffers) this.notifyBuffers();
    if (hadStreaming) this.notifyStreaming();
  }

  private evictExpired(now: number): boolean {
    let changed = false;
    for (const [id, entry] of this.map) {
      if (now - entry.lastAccess > BUFFER_TTL_MS) {
        this.map.delete(id);
        this.streaming.delete(id);
        changed = true;
      }
    }
    return changed;
  }

  private snapshotBuffers(): BuffersSnapshot {
    const out: BuffersSnapshot = {};
    for (const [id, entry] of this.map) out[id] = entry.text;
    return out;
  }

  private notifyBuffers(): void {
    const snap = this.snapshotBuffers();
    for (const fn of this.buffersSubs) fn(snap);
  }

  private notifyStreaming(): void {
    const snap = new Set(this.streaming);
    for (const fn of this.streamingSubs) fn(snap);
  }
}

/** Singleton backing the public API. */
export const tokenBufferStore = new TokenBufferStore();

/** Svelte-readable snapshot of per-session token buffers. */
export const globalTokenBuffers: Readable<BuffersSnapshot> = {
  subscribe: (fn) => tokenBufferStore.subscribeBuffers(fn),
};

/** Svelte-readable set of session IDs actively streaming tokens. */
export const globalStreamingSessions: Readable<StreamingSnapshot> = {
  subscribe: (fn) => tokenBufferStore.subscribeStreaming(fn),
};

export function appendGlobalToken(sessionId: string, token: string): void {
  tokenBufferStore.append(sessionId, token);
}

export function clearGlobalBuffer(sessionId: string): void {
  tokenBufferStore.clear(sessionId);
}

export function closeSessionBuffer(sessionId: string): void {
  tokenBufferStore.closeSession(sessionId);
}

export function getBuffer(sessionId: string): string {
  return tokenBufferStore.get(sessionId);
}
