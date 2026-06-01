/**
 * Client-side chat send rate-limiter.
 *
 * Enforces two thresholds:
 * - Min interval between sends: 500 ms (debounce against accidental double-Enter).
 * - Max sends per rolling minute: 30 (soft client guard, not a security boundary).
 *
 * Stateless pure class - one instance per ChatInput. Backend-side throttling
 * remains authoritative; this exists to give immediate UX feedback.
 */
export interface RateLimitResult {
  allowed: boolean;
  /** Human-readable reason when `allowed === false`. */
  reason?: "too_fast" | "too_many";
  /** Milliseconds to wait before the next attempt would succeed. */
  retryAfterMs?: number;
}

export class ChatRateLimiter {
  private readonly minIntervalMs: number;
  private readonly maxPerMinute: number;
  private lastSendAt = 0;
  private recent: number[] = [];

  constructor(minIntervalMs = 500, maxPerMinute = 30) {
    this.minIntervalMs = minIntervalMs;
    this.maxPerMinute = maxPerMinute;
  }

  /** Check without recording - use before disabling submit. */
  check(now = Date.now()): RateLimitResult {
    const sinceLast = now - this.lastSendAt;
    if (sinceLast < this.minIntervalMs) {
      return {
        allowed: false,
        reason: "too_fast",
        retryAfterMs: this.minIntervalMs - sinceLast,
      };
    }
    this.prune(now);
    if (this.recent.length >= this.maxPerMinute) {
      const oldest = this.recent[0];
      return {
        allowed: false,
        reason: "too_many",
        retryAfterMs: 60_000 - (now - oldest),
      };
    }
    return { allowed: true };
  }

  /** Record a send. Call only after `check()` returns allowed. */
  record(now = Date.now()): void {
    this.lastSendAt = now;
    this.recent.push(now);
    this.prune(now);
  }

  reset(): void {
    this.lastSendAt = 0;
    this.recent = [];
  }

  private prune(now: number): void {
    const cutoff = now - 60_000;
    while (this.recent.length > 0 && this.recent[0] < cutoff) {
      this.recent.shift();
    }
  }
}
