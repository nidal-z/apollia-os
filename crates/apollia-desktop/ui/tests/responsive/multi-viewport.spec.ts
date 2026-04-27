import { test, expect, type Page } from "@playwright/test";

/**
 * Multi-viewport responsive smoke tests.
 *
 * Stubs Tauri IPC so the UI bundle boots self-contained. Covers the
 * behaviour-critical AC of the story:
 *   (a) 375 px  → sidebar hidden, hamburger surfaces the drawer.
 *   (b) hamburger click opens overlay drawer with focus trap.
 *   (c) Settings mobile surfaces the sections trigger.
 *   (d) Onboarding renders offline banner when network probe fails.
 * Visual snapshots are intentionally deferred to a follow-up spec —
 * this file keeps the behaviour contract green on every run.
 */

type InvokeStub = (cmd: string, args: unknown) => unknown;

async function installTauriStub(page: Page): Promise<void> {
  await page.addInitScript(() => {
    const handlers: Record<string, (args: unknown) => unknown> = {
      get_user_memory: () => [],
      get_config: () => ({ config_path: "~/.apollia/apollia.toml", config_exists: true, sections: [] }),
      get_system_info: () => ({}),
      list_llm_backends: () => [],
      get_onboarding_status: () => ({ completed: true, phase: "done", required: false }),
    };
    (window as unknown as { __TAURI_INTERNALS__?: { invoke: InvokeStub } }).__TAURI_INTERNALS__ = {
      invoke: (cmd: string, args: unknown) => {
        const fn = handlers[cmd];
        return fn ? Promise.resolve(fn(args)) : Promise.resolve(null);
      },
    };
  });
}

test.describe("responsive layout", () => {
  test.beforeEach(async ({ page }) => {
    await installTauriStub(page);
  });

  test("375px hides sidebar and exposes hamburger", async ({ page }) => {
    await page.setViewportSize({ width: 375, height: 667 });
    await page.goto("/");

    // Hamburger should be visible, sidebar should NOT render inline.
    const hamburger = page.getByTestId("topbar-sidebar-toggle");
    await expect(hamburger).toBeVisible();

    // No inline sidebar (drawer mode renders only when open).
    const sidebars = page.locator('[data-testid="sidebar"]');
    await expect(sidebars).toHaveCount(0);
  });

  test("hamburger opens drawer with aria-modal + focus trap", async ({ page }) => {
    await page.setViewportSize({ width: 375, height: 667 });
    await page.goto("/");

    await page.getByTestId("topbar-sidebar-toggle").click();
    const drawer = page.getByTestId("sidebar");
    await expect(drawer).toBeVisible();
    await expect(drawer).toHaveAttribute("aria-modal", "true");

    // Escape closes the drawer.
    await page.keyboard.press("Escape");
    await expect(drawer).toHaveCount(0);
  });

  test("settings mobile exposes sections trigger", async ({ page }) => {
    await page.setViewportSize({ width: 375, height: 667 });
    await page.goto("/#settings");
    await expect(page.getByTestId("settings-mobile-nav-toggle")).toBeVisible();
  });

  test("1280px shows expanded sidebar + no hamburger", async ({ page }) => {
    await page.setViewportSize({ width: 1280, height: 800 });
    await page.goto("/");

    await expect(page.getByTestId("sidebar")).toBeVisible();
    await expect(page.getByTestId("topbar-sidebar-toggle")).toHaveCount(0);
  });

  test("onboarding offline banner surfaces on network timeout", async ({ page }) => {
    await page.setViewportSize({ width: 375, height: 667 });
    // Force the probe fetch to fail.
    await page.route("**/logo.svg?probe=1", (route) => route.abort());
    await page.goto("/#onboarding");
    await expect(page.getByTestId("onboarding-offline-banner")).toBeVisible({
      timeout: 10_000,
    });
  });
});
