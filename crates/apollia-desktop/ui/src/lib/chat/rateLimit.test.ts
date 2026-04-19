import { describe, it, expect } from "vitest";
import { ChatRateLimiter } from "./rateLimit";

describe("ChatRateLimiter", () => {
  it("allows the first send immediately", () => {
    const rl = new ChatRateLimiter();
    expect(rl.check(1_000).allowed).toBe(true);
  });

  it("blocks a second send within the min interval", () => {
    const rl = new ChatRateLimiter(500, 30);
    rl.record(1_000);
    const res = rl.check(1_100);
    expect(res.allowed).toBe(false);
    expect(res.reason).toBe("too_fast");
    expect(res.retryAfterMs).toBe(400);
  });

  it("allows sends after the min interval has elapsed", () => {
    const rl = new ChatRateLimiter(500, 30);
    rl.record(1_000);
    expect(rl.check(1_500).allowed).toBe(true);
  });

  it("caps at maxPerMinute", () => {
    const rl = new ChatRateLimiter(0, 3);
    rl.record(1_000);
    rl.record(2_000);
    rl.record(3_000);
    const res = rl.check(4_000);
    expect(res.allowed).toBe(false);
    expect(res.reason).toBe("too_many");
  });

  it("evicts old entries after the 60 s window", () => {
    const rl = new ChatRateLimiter(0, 2);
    rl.record(1_000);
    rl.record(2_000);
    // 61 s later — both old entries are evicted.
    expect(rl.check(62_001).allowed).toBe(true);
  });
});
