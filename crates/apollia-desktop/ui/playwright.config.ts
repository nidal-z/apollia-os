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
 * run alone (`playwright test --project=perf`, or by path filter as
 * `npm run test:perf` does).  `scripts/check_playwright_specs.py` fails when
 * a spec lands outside every `testDir` declared here.
 */
export default defineConfig({
  testDir: "./tests",
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
      name: "perf",
      testDir: "./tests/perf",
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
  webServer: {
    command: "npm run preview -- --port 4173",
    url: "http://localhost:4173",
    reuseExistingServer: true,
    timeout: 120_000,
  },
});
