import { defineConfig, devices } from "@playwright/test";

// The port of the preview server. Two worktrees running their suites at once
// each name their own, since neither may answer for the other's bundle.
const port = Number(process.env.APOLLIA_PLAYWRIGHT_PORT ?? 4173);

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
    baseURL: `http://localhost:${port}`,
    trace: "on-first-retry",
    viewport: { width: 1280, height: 800 },
  },
  projects: [
    {
      name: "agents",
      testDir: "./tests/agents",
      use: { ...devices["Desktop Chrome"] },
    },
    {
      name: "companion",
      testDir: "./tests/companion",
      use: { ...devices["Desktop Chrome"] },
    },
    {
      name: "connections",
      testDir: "./tests/connections",
      use: { ...devices["Desktop Chrome"] },
    },
    {
      name: "library",
      testDir: "./tests/library",
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
  // build is therefore part of starting the server, and a server already
  // listening on the port is never reused: it serves whatever `dist/` it was
  // started on, a bundle older than the sources or another worktree's, and
  // the run would measure that instead. An occupied port fails the run at
  // once, naming the port; `--strictPort` keeps the preview from moving to
  // the next free one while the tests still ask the first.
  webServer: {
    command: `npm run build && npm run preview -- --port ${port} --strictPort`,
    url: `http://localhost:${port}`,
    reuseExistingServer: false,
    timeout: 120_000,
  },
});
