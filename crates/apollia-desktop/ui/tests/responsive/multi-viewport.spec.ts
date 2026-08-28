import { test, expect, type Page } from "@playwright/test";

/**
 * Multi-viewport responsive smoke tests.
 *
 * Stubs Tauri IPC so the UI bundle boots self-contained. Covers the
 * behaviour-critical responsive contracts:
 *   (a) 375 px  → sidebar hidden, hamburger surfaces the drawer.
 *   (b) hamburger click opens overlay drawer with focus trap.
 *   (c) Settings mobile surfaces the sections trigger, skipped under the
 *       condition written on the case itself.
 * Visual snapshots are intentionally deferred to a follow-up spec.
 *
 * The stub below is what the whole corpus needs and only this file carries:
 * `@tauri-apps/api/event` routes every `listen()` through
 * `__TAURI_INTERNALS__.transformCallback` (node_modules/@tauri-apps/api/core.js),
 * and the boot path feeds `list_*` results straight into stores that iterate
 * them. An `invoke`-only stub returning `null` therefore leaves the bundle on
 * `data-testid=app-loading` for good, with nothing logged.
 */

type InvokeStub = (cmd: string, args: unknown) => unknown;

/** The three members of the Tauri v2 bridge this bundle reaches for at boot. */
interface TauriInternalsStub {
  invoke: InvokeStub;
  transformCallback: (cb: (payload: unknown) => void, once: boolean) => number;
  convertFileSrc: (path: string) => string;
}

async function installTauriStub(page: Page): Promise<void> {
  await page.addInitScript(() => {
    const handlers: Record<string, (args: unknown) => unknown> = {
      get_user_memory: () => [],
      get_config: () => ({ config_path: "~/.apollia/apollia.toml", config_exists: true, sections: [] }),
      get_system_info: () => ({}),
      list_llm_backends: () => [],
    };
    let nextCallbackId = 1;
    (globalThis as unknown as { __TAURI_INTERNALS__?: TauriInternalsStub }).__TAURI_INTERNALS__ = {
      invoke: (cmd: string, args: unknown) => {
        const fn = handlers[cmd];
        if (fn) return Promise.resolve(fn(args));
        // A store that receives `null` where it expects a list throws inside a
        // Svelte effect, which kills the render loop silently.
        if (cmd.startsWith("list_")) return Promise.resolve([]);
        return Promise.resolve(null);
      },
      transformCallback: (cb: (payload: unknown) => void, once: boolean) => {
        const id = nextCallbackId++;
        Object.defineProperty(globalThis, `_${id}`, {
          value: (payload: unknown) => {
            if (once) Reflect.deleteProperty(globalThis, `_${id}`);
            return cb(payload);
          },
          writable: false,
          configurable: true,
        });
        return id;
      },
      convertFileSrc: (path: string) => path,
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
    // The other three cases of this file run; this one cannot, and the reason
    // is the corpus-wide one carried by every settings spec.
    test.skip(
      true,
      "runs again once this case reaches the settings page through a gesture " +
        "the application supports: nothing in src/ reads a '#settings' " +
        "fragment, navigation goes through the currentRoute store " +
        "(src/lib/stores/navigation.ts), so the page under test never mounts",
    );
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
});
