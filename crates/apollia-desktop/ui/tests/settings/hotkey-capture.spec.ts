import { test, expect } from "@playwright/test";

/**
 * STT settings refonte + Hotkey capture modal.
 *
 * Covers the fullscreen hotkey-capture dialog: preview chips while keys are
 * pressed, timeout auto-cancel with a "Capture cancelled" toast, and the
 * inline collision warning when the captured combo matches a reserved OS
 * shortcut (e.g. Cmd+C).
 */

type InvokeStub = (cmd: string, args: unknown) => unknown;

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
      get_stt_status: () => ({
        enabled: true,
        model_loaded: true,
        model_name: "tiny",
        backend_name: "whisper-rs",
        metal_enabled: false,
        cuda_enabled: false,
      }),
      update_stt_config: (args: unknown) => {
        Object.assign(sttConfig, (args as { config: typeof sttConfig }).config);
        return null;
      },
      get_config: () => ({ config_path: "~/.apollia/apollia.toml", config_exists: true, sections: [] }),
      get_system_info: () => ({}),
      get_cli_status: () => ({}),
      list_llm_backends: () => [],
    };

    (window as unknown as { __TAURI_INTERNALS__?: { invoke: InvokeStub } }).__TAURI_INTERNALS__ = {
      invoke: (cmd: string, args: unknown) => {
        const h = handlers[cmd];
        return Promise.resolve(h ? h(args) : null);
      },
    };
  });
}

test.describe("STT hotkey capture dialog", () => {
  test.beforeEach(async ({ page }) => {
    await installTauriStub(page);
    await page.goto("/?route=settings&sub=stt");
    await page.waitForSelector('[data-testid="stt-config-form"]');
  });

  test("opens capture modal and shows 3 chips on Cmd+Shift+P", async ({ page }) => {
    await page.locator('[data-testid="stt-hotkey-change-btn"]').click();
    const dialog = page.locator('[data-testid="hotkey-capture-dialog"]');
    await expect(dialog).toBeVisible();

    // Press Cmd+Shift+P via the low-level keyboard API so modifiers are held.
    await page.keyboard.down("Meta");
    await page.keyboard.down("Shift");
    await page.keyboard.down("KeyP");

    const chips = page.locator('[data-testid="hotkey-capture-chips"] kbd');
    await expect(chips).toHaveCount(3);

    await page.keyboard.up("KeyP");
    await page.keyboard.up("Shift");
    await page.keyboard.up("Meta");
  });

  test("modal auto-closes after 15s timeout and shows cancelled toast", async ({ page }) => {
    // Use a clock-fast-forward: override timeoutMs via the dialog's prop is
    // not directly exposed, so we rely on the real timer but bump test timeout.
    test.slow();
    await page.locator('[data-testid="stt-hotkey-change-btn"]').click();
    await expect(page.locator('[data-testid="hotkey-capture-dialog"]')).toBeVisible();

    await page.waitForSelector('[data-testid="hotkey-capture-dialog"]', { state: "detached", timeout: 20_000 });
    // Toast surface: any toast with "cancelled" text.
    await expect(page.getByText(/cancelled|annulée/i).first()).toBeVisible({ timeout: 3_000 });
  });

  test("Cmd+C shows collision warning but Confirm remains enabled", async ({ page }) => {
    await page.locator('[data-testid="stt-hotkey-change-btn"]').click();
    await expect(page.locator('[data-testid="hotkey-capture-dialog"]')).toBeVisible();

    await page.keyboard.down("Meta");
    await page.keyboard.down("KeyC");

    await expect(page.locator('[data-testid="hotkey-capture-collision"]')).toBeVisible();
    await expect(page.locator('[data-testid="hotkey-capture-confirm"]')).toBeEnabled();

    await page.keyboard.up("KeyC");
    await page.keyboard.up("Meta");
  });
});
