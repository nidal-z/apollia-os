import { test, expect } from "@playwright/test";

/**
 * Seeds a synthetic conversation of 500 messages in-memory via a
 * sessionStorage bridge (the app reads it at boot when the flag is set) and
 * measures the scroll frame rate over a 5-second window.
 *
 * Target: average FPS ≥ 55 on a mid-tier CPU.  The test uses
 * `requestAnimationFrame` sampling to compute frame durations.
 */
test.describe("chat message list — 500 messages", () => {
  test("sustains ≥ 55 fps while scrolling", async ({ page }) => {
    await page.goto("/");
    await page.evaluate(() => {
      const msgs = Array.from({ length: 500 }, (_, i) => ({
        id: `m-${i}`,
        role: i % 2 === 0 ? "user" : "assistant",
        content: `Synthetic message ${i} with enough characters to render a realistic bubble line.`,
        tool_calls: null,
        tool_name: null,
        seq: i,
        created_at: new Date(Date.now() - (500 - i) * 60_000).toISOString(),
      }));
      window.sessionStorage.setItem(
        "apollia.perftest.messages",
        JSON.stringify(msgs),
      );
    });

    await page.waitForSelector("[data-testid^='chat-message-']", { timeout: 10_000 });

    const fps = await page.evaluate<number>(async () => {
      return new Promise<number>((resolve) => {
        const frames: number[] = [];
        let last = performance.now();
        const start = last;
        const DURATION = 5_000;

        function tick() {
          const now = performance.now();
          frames.push(now - last);
          last = now;
          if (now - start < DURATION) {
            window.scrollBy(0, 8);
            requestAnimationFrame(tick);
          } else {
            const avg = frames.reduce((a, b) => a + b, 0) / frames.length;
            resolve(1000 / avg);
          }
        }
        requestAnimationFrame(tick);
      });
    });

    expect(fps).toBeGreaterThanOrEqual(55);
  });
});
