import { test, expect } from "@playwright/test";

/**
 * Streams 10 000 synthetic tokens through the StreamingMessage path and
 * measures the p95 frame time.  Target: p95 ≤ 18 ms (≥ 55 fps sustained).
 */
test.describe("chat streaming - 10k tokens", () => {
  test("keeps p95 frame time ≤ 18 ms", async ({ page }) => {
    await page.goto("/");

    // Flag enabling the streaming harness in the built UI.  The harness is
    // only active when `?perf=stream` is present (guards against production
    // accidentally exposing it).
    await page.goto("/?perf=stream");

    await page.waitForSelector("[data-testid='chat-message-streaming']", {
      timeout: 10_000,
    });

    const p95 = await page.evaluate<number>(async () => {
      return new Promise<number>((resolve) => {
        const frameTimes: number[] = [];
        let last = performance.now();
        const start = last;
        const DURATION = 5_000;

        function sample() {
          const now = performance.now();
          frameTimes.push(now - last);
          last = now;
          if (now - start < DURATION) {
            requestAnimationFrame(sample);
          } else {
            frameTimes.sort((a, b) => a - b);
            const idx = Math.floor(frameTimes.length * 0.95);
            resolve(frameTimes[idx] ?? 0);
          }
        }
        requestAnimationFrame(sample);
      });
    });

    expect(p95).toBeLessThanOrEqual(18);
  });
});
