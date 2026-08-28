import { defineConfig, devices } from "@playwright/test";

/**
 * Playwright config for the desktop UI browser suites.
 *
 * These tests run against the *UI bundle only* (no Tauri backend); Tauri
 * commands are stubbed via `window.__TAURI_INTERNALS__` overrides in each
 * spec.  This keeps CI fast and avoids pulling the Rust binary into the
 * test matrix.
 *
 * One project per subdirectory of `tests/`, so every spec is reachable by
 * the runner (`playwright test --list` names them all) and a subset can be
 * run alone (`playwright test --project=settings`, or by path filter).
 * `scripts/check_playwright_specs.py` fails when a spec lands outside every
 * `testDir` declared here.
 *
 * A project is removed only together with the specs it carried: an entry
 * naming an empty directory makes `--project=<name>` answer "0 tests" instead
 * of failing, which is the same silence the guard exists to break.
 */
export default defineConfig({
  testDir: "./tests",
  // Reasons for every skipped case, printed before the run: the `list`
  // reporter renders a skip as a dash and drops its annotation, which is the
  // same silence as a red suite nobody reads. See `tests/skip-conditions.ts`.
  globalSetup: "./tests/skip-conditions.ts",
  timeout: 60_000,
  expect: { timeout: 5_000 },
  fullyParallel: false,
  reporter: [["list"]],
  use: {
    baseURL: "http://localhost:4173",
    trace: "on-first-retry",
    viewport: { width: 1280, height: 800 },
  },
  projects: [
    {
      name: "companion",
      testDir: "./tests/companion",
      use: { ...devices["Desktop Chrome"] },
    },
    {
      name: "responsive",
      testDir: "./tests/responsive",
      use: { ...devices["Desktop Chrome"] },
    },
    {
      name: "settings",
      testDir: "./tests/settings",
      use: { ...devices["Desktop Chrome"] },
    },
  ],
  // `vite preview` serves `dist/`, and on a tree that has never been built
  // there is no `dist/`: the server exits at once and the run answers
  // "Timed out waiting 120000ms from config.webServer" two minutes later,
  // naming neither the missing bundle nor the command that produces it. The
  // build is therefore part of starting the server. `reuseExistingServer`
  // skips both when a preview is already up.
  webServer: {
    command: "npm run build && npm run preview -- --port 4173",
    url: "http://localhost:4173",
    reuseExistingServer: true,
    timeout: 120_000,
  },
});
