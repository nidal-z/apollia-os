import { defineConfig, devices } from "@playwright/test";

/**
 * Playwright config for chat performance tests (US-SP42-035, Phase 10).
 *
 * These tests run against the *UI bundle only* (no Tauri backend); Tauri
 * commands are stubbed via `window.__TAURI_INTERNALS__` overrides in each
 * spec.  This keeps CI fast and avoids pulling the Rust binary into the
 * test matrix.
 */
export default defineConfig({
  testDir: "./tests/perf",
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
      name: "chromium",
      use: { ...devices["Desktop Chrome"] },
    },
  ],
  webServer: {
    command: "pnpm preview --port 4173",
    url: "http://localhost:4173",
    reuseExistingServer: true,
    timeout: 120_000,
  },
});
