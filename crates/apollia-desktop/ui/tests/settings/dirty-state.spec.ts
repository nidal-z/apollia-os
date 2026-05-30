import { test, expect } from "@playwright/test";

/**
 * Settings dirty-state tracking, Cmd+S save and nav-guard dialog.
 *
 * These tests run against the production UI bundle with Tauri commands
 * stubbed via `window.__TAURI_INTERNALS__.invoke`. The stubs return deterministic
 * payloads so we can focus on the UI's dirty-state machinery.
 */

type InvokeStub = (cmd: string, args: unknown) => unknown;

// Install a Tauri stub before any app code runs. Each spec specialises the
// `invokeHandler` below via page.addInitScript.
async function installTauriStub(page: import("@playwright/test").Page): Promise<void> {
  await page.addInitScript(() => {
    const sttConfig = {
      enabled: true,
      model_path: "~/.apollia/models/tiny.bin",
      language: "",
      hotkey: "ctrl+shift+space",
      trigger_mode: "toggle",
      clipboard_mode: "paste",
      max_recording_sec: 60,
      silence_threshold_db: -40,
      clipboard_restore: true,
    };

    const handlers: Record<string, (args: unknown) => unknown> = {
      get_stt_config: () => sttConfig,
      list_stt_models: () => [],
      get_stt_status: () => ({ enabled: true, model_loaded: true, model_name: "tiny", backend_name: "whisper-rs", metal_enabled: false, cuda_enabled: false }),
      update_stt_config: (args: unknown) => {
        Object.assign(sttConfig, (args as { config: typeof sttConfig }).config);
        return null;
      },
      get_config: () => ({ config_path: "~/.apollia/apollia.toml", config_exists: true, sections: [] }),
      get_system_info: () => ({}),
      get_cli_status: () => ({}),
      list_llm_backends: () => [],
    };

    // Minimal Tauri-v2 invoke shim — only what the UI calls.
    (globalThis as unknown as { __TAURI_INTERNALS__?: { invoke: InvokeStub } }).__TAURI_INTERNALS__ = {
      invoke: (cmd: string, args: unknown) => {
        const h = handlers[cmd];
        return Promise.resolve(h ? h(args) : null);
      },
    };
  });
}

test.describe("settings dirty-state", () => {
  test.beforeEach(async ({ page }) => {
    await installTauriStub(page);
  });

  test("STT: edit hotkey, Cmd+S saves, status badge shows 'Saved'", async ({ page }) => {
    await page.goto("/?route=settings&sub=stt");

    // Wait for form to mount.
    await page.waitForSelector('[data-testid="stt-config-form"]');
    await page.waitForSelector('[data-testid="stt-hotkey-input"]');

    // Mutate the language field — easier to drive than a hotkey capture.
    const langInput = page.locator('[data-testid="stt-language-input"]');
    await langInput.fill("en");

    await expect(page.locator('[data-testid="settings-unsaved-badge"]')).toBeVisible();

    // Trigger Cmd+S (metaKey on macOS, ctrlKey elsewhere).
    // NOSONAR typescript:S1874 — navigator.platform retained inside page.evaluate;
    // userAgentData is unavailable in WebKit (Playwright cross-browser parity).
    const isMac = await page.evaluate(() => /Mac/.test(navigator.platform)); // NOSONAR typescript:S1874
    await page.keyboard.press(isMac ? "Meta+s" : "Control+s");

    await expect(page.locator('[data-testid="stt-save-status-saved"]')).toBeVisible({ timeout: 3_000 });
    await expect(page.locator('[data-testid="settings-unsaved-badge"]')).toBeHidden();
  });

  test("nav with dirty form opens 3-option dialog (Discard / Save & Continue / Stay)", async ({ page }) => {
    await page.goto("/?route=settings&sub=stt");
    await page.waitForSelector('[data-testid="stt-config-form"]');
    await page.locator('[data-testid="stt-language-input"]').fill("fr");

    // Attempt to navigate to another sub-route — the nav should be blocked.
    await page.locator('[data-testid="settings-nav-llm"]').click();

    const dialog = page.locator('[data-testid="settings-unsaved-dialog"]');
    await expect(dialog).toBeVisible();
    await expect(dialog.locator('[data-testid="settings-unsaved-dialog-discard"]')).toBeVisible();
    await expect(dialog.locator('[data-testid="settings-unsaved-dialog-save"]')).toBeVisible();
    await expect(dialog.locator('[data-testid="settings-unsaved-dialog-stay"]')).toBeVisible();

    // Default focus is "Stay" — pressing Enter keeps us here.
    await page.keyboard.press("Enter");
    await expect(dialog).toBeHidden();
    await expect(page.locator('[data-testid="stt-config-form"]')).toBeVisible();
  });

  test("scroll position is restored when switching back to a sub-route", async ({ page }) => {
    await page.goto("/?route=settings&sub=stt");
    await page.waitForSelector('[data-testid="stt-config-form"]');

    const container = page.locator("[data-settings-scroll]");
    await container.evaluate((el) => { (el as HTMLElement).scrollTop = 240; });
    const saved = await container.evaluate((el) => (el as HTMLElement).scrollTop);

    await page.locator('[data-testid="settings-nav-llm"]').click();
    await page.waitForSelector('[data-testid="llm-backends-section"], [data-testid="llm-backends-empty"]');

    await page.locator('[data-testid="settings-nav-stt"]').click();
    await page.waitForSelector('[data-testid="stt-config-form"]');

    // Allow requestAnimationFrame + tick to restore the scroll.
    await page.waitForTimeout(100);
    const restored = await container.evaluate((el) => (el as HTMLElement).scrollTop);
    expect(restored).toBe(saved);
  });
});
