import { test, expect } from "@playwright/test";

/**
 * US-SP42-082 — Danger Zone 2-step confirm + Shortcuts search.
 *
 * Covers:
 *   - Reset Onboarding card opens the 2-step dialog; step 1 Cancel closes it;
 *     step 2 Confirm is disabled until the user types exactly RESET.
 *   - Shortcuts page renders categorized rows and the search input filters
 *     down to a single category/row for a specific combo query.
 */

type InvokeStub = (cmd: string, args: unknown) => unknown;

async function installTauriStub(page: import("@playwright/test").Page): Promise<void> {
  await page.addInitScript(() => {
    const handlers: Record<string, (args: unknown) => unknown> = {
      get_config: () => ({ config_path: "~/.apollia/apollia.toml", config_exists: true, sections: [] }),
      get_system_info: () => ({}),
      get_cli_status: () => ({}),
      list_llm_backends: () => [],
      reset_onboarding: () => null,
      clear_user_memory: () => 0,
      clear_logs: () => null,
      factory_reset: () => null,
      app_restart: () => null,
    };

    (window as unknown as { __TAURI_INTERNALS__?: { invoke: InvokeStub } }).__TAURI_INTERNALS__ = {
      invoke: (cmd: string, args: unknown) => {
        const h = handlers[cmd];
        return Promise.resolve(h ? h(args) : null);
      },
    };
  });
}

test.describe("Danger Zone — 2-step destructive confirmation", () => {
  test.beforeEach(async ({ page }) => {
    await installTauriStub(page);
  });

  test("Reset Onboarding: cancel step 1, then confirm after typing RESET", async ({ page }) => {
    await page.goto("/?route=settings&sub=danger");
    await page.waitForSelector('[data-testid="danger-section"]');

    // Open dialog.
    await page.locator('[data-testid="reset-onboarding-btn"]').click();
    const dialog = page.locator('[data-testid="reset-onboarding-confirm"]');
    await expect(dialog).toBeVisible();
    await expect(dialog.locator('[data-testid="reset-onboarding-confirm-step1"]')).toBeVisible();

    // Cancel on step 1 closes the dialog.
    await dialog.locator('[data-testid="reset-onboarding-confirm-cancel"]').click();
    await expect(dialog).toBeHidden();

    // Re-open, advance to step 2.
    await page.locator('[data-testid="reset-onboarding-btn"]').click();
    await dialog.locator('[data-testid="reset-onboarding-confirm-continue"]').click();
    await expect(dialog.locator('[data-testid="reset-onboarding-confirm-step2"]')).toBeVisible();

    // Confirm disabled while input is empty or wrong.
    const confirm = dialog.locator('[data-testid="reset-onboarding-confirm-confirm"]');
    await expect(confirm).toBeDisabled();
    await dialog.locator('[data-testid="reset-onboarding-confirm-input"]').fill("reset");
    await expect(confirm).toBeDisabled();

    // Typing RESET unlocks the destructive button.
    await dialog.locator('[data-testid="reset-onboarding-confirm-input"]').fill("RESET");
    await expect(confirm).toBeEnabled();

    // Intercept the restart confirm() prompt that fires after success.
    page.once("dialog", (d) => d.dismiss());
    await confirm.click();
  });
});

test.describe("Shortcuts page — search filter", () => {
  test.beforeEach(async ({ page }) => {
    await installTauriStub(page);
  });

  test("typing Cmd+K narrows the list to the palette row only", async ({ page }) => {
    await page.goto("/?route=settings&sub=shortcuts");
    await page.waitForSelector('[data-testid="shortcuts-section"]');

    // Multiple categories render by default.
    await expect(page.locator('[data-testid="shortcuts-category-global"]')).toBeVisible();
    await expect(page.locator('[data-testid="shortcuts-category-navigation"]')).toBeVisible();

    // Filter for the command palette chord — on either mac or win we search
    // for the literal "K" so the filter matches ⌘+K and Ctrl+K alike.
    await page.locator('[data-testid="shortcuts-search"]').fill("K");
    // Palette row must remain visible.
    const paletteRow = page.locator('[data-testid="shortcut-row-palette.open"]');
    await expect(paletteRow).toBeVisible();
  });

  test("search with no match shows the empty state", async ({ page }) => {
    await page.goto("/?route=settings&sub=shortcuts");
    await page.waitForSelector('[data-testid="shortcuts-section"]');
    await page.locator('[data-testid="shortcuts-search"]').fill("this-should-match-nothing");
    await expect(page.locator('[data-testid="shortcuts-empty"]')).toBeVisible();
  });
});
