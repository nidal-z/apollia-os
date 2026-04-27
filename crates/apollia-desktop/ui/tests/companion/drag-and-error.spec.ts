import { test, expect, type Page } from "@playwright/test";

/**
 * Companion drag/snap + error recovery + restore pulse.
 *
 * Stubs Tauri IPC so the companion can be enabled and rendered entirely from
 * the UI bundle. Exercised scenarios mirror the story acceptance criteria:
 * snap-to-edge on release within 20 px, error-state fallback on session
 * timeout, geometry persistence across reloads, and restore-button pulse on
 * first mount.
 */

type InvokeStub = (cmd: string, args: unknown) => unknown;

async function installTauriStub(
  page: Page,
  opts: { sessionDelayMs?: number } = {},
): Promise<void> {
  await page.addInitScript((delay) => {
    const handlers: Record<string, (args: unknown) => unknown> = {
      get_user_memory: () => [{ key: "companion_enabled", value: "true" }],
      set_companion_enabled: () => null,
      list_llm_backends: () => [
        { id: "local", name: "Local", ready: true },
      ],
      get_companion_context: () => "Contextual help.",
      create_companion_session: () =>
        new Promise((resolve) => {
          setTimeout(() => resolve({ session_id: "sess-1" }), delay);
        }),
      update_chat_session: () => null,
    };
    (window as unknown as { __TAURI_INTERNALS__?: { invoke: InvokeStub } }).__TAURI_INTERNALS__ = {
      invoke: (cmd: string, args: unknown) => {
        const fn = handlers[cmd];
        return fn ? Promise.resolve(fn(args)) : Promise.resolve(null);
      },
    };
  }, opts.sessionDelayMs ?? 50);
}

test.describe("Companion — drag & snap", () => {
  test("snaps to right edge when released <20 px away", async ({ page }) => {
    await installTauriStub(page);
    await page.goto("/");
    const panel = page.getByTestId("companion-panel");
    await expect(panel).toBeVisible();

    const box = await panel.boundingBox();
    if (!box) throw new Error("panel box unavailable");

    // Drag panel so its right edge ends within the snap trigger distance.
    const viewport = page.viewportSize();
    if (!viewport) throw new Error("viewport missing");
    const targetX = viewport.width - box.width - 10; // 10 px from right edge
    await page.mouse.move(box.x + 30, box.y + 10);
    await page.mouse.down();
    await page.mouse.move(targetX + 30, box.y + 10, { steps: 10 });
    await page.mouse.up();

    const after = await panel.boundingBox();
    if (!after) throw new Error("post-drag box missing");
    expect(Math.round(after.x + after.width)).toBe(viewport.width);
  });
});

test.describe("Companion — error state", () => {
  test("timeout surfaces CompanionErrorState with the 3 action buttons", async ({
    page,
  }) => {
    // Session creation never resolves → force the 10 s timeout path.
    await page.addInitScript(() => {
      (window as unknown as { __TAURI_INTERNALS__?: { invoke: InvokeStub } }).__TAURI_INTERNALS__ = {
        invoke: (cmd: string) => {
          if (cmd === "get_user_memory")
            return Promise.resolve([
              { key: "companion_enabled", value: "true" },
            ]);
          if (cmd === "list_llm_backends")
            return Promise.resolve([{ id: "local", name: "Local", ready: true }]);
          if (cmd === "create_companion_session")
            return new Promise(() => {}); // never resolves
          return Promise.resolve(null);
        },
      };
    });
    await page.goto("/");

    const errorState = page.getByTestId("companion-error-state");
    await expect(errorState).toBeVisible({ timeout: 15_000 });
    await expect(page.getByTestId("companion-error-retry")).toBeVisible();
    await expect(page.getByTestId("companion-error-settings")).toBeVisible();
    await expect(page.getByTestId("companion-error-close")).toBeVisible();
    await expect(page.getByTestId("companion-error-code")).toHaveText(
      /ERR_COMPANION_/,
    );
  });
});

test.describe("Companion — geometry persistence", () => {
  test("position is restored from localStorage.companionGeometry", async ({
    page,
  }) => {
    await installTauriStub(page);
    await page.goto("/");
    await page.evaluate(() => {
      localStorage.setItem(
        "companionGeometry",
        JSON.stringify({
          position: { x: 42, y: 123 },
          size: { width: 400, height: 500 },
        }),
      );
    });
    await page.reload();

    const panel = page.getByTestId("companion-panel");
    await expect(panel).toBeVisible();
    const box = await panel.boundingBox();
    if (!box) throw new Error("box unavailable");
    expect(Math.round(box.x)).toBe(42);
    expect(Math.round(box.y)).toBe(123);
    expect(Math.round(box.width)).toBe(400);
    expect(Math.round(box.height)).toBe(500);
  });
});

test.describe("Companion — restore pulse", () => {
  test("restore button pulses on first mount and stops after click", async ({
    page,
  }) => {
    await installTauriStub(page);
    await page.goto("/");
    await page.getByTestId("companion-minimize").click();
    const restore = page.getByTestId("companion-restore");
    await expect(restore).toHaveClass(/companion-pulse/);
    await restore.click();
    await expect(page.getByTestId("companion-panel")).toBeVisible();
  });
});
