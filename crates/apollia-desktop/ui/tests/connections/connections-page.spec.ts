import { test, expect, type Page } from "@playwright/test";

/**
 * The Connections page: a summary over a grid of tiles, the card of the
 * chosen connection beside it.
 *
 * Boots the UI bundle with the Tauri bridge stubbed (the three members the
 * boot path reaches for, as in `add-connection.spec.ts`). The stub answers
 * three connections (a folder never checked, a service brought by a package,
 * a Google account) and fails the state of the services, so the page shows
 * what needs repair first, a banner for the one source that failed, and keeps
 * its card, search and chip when the user leaves for another screen.
 */

type InvokeStub = (cmd: string, args: unknown) => unknown;

interface TauriInternalsStub {
  invoke: InvokeStub;
  transformCallback: (cb: (payload: unknown) => void, once: boolean) => number;
  convertFileSrc: (path: string) => string;
}

async function installTauriStub(page: Page): Promise<void> {
  await page.addInitScript(() => {
    const base = {
      provenance: "selection",
      official: false,
      state: "connected",
      tools: [],
    };
    const connections = [
      {
        ...base,
        id: "mcp:notion",
        kind: "mcp",
        label: "notion",
        account: null,
        remote: true,
        status: "working",
        reason: null,
        checkedAt: new Date(Date.now() - 5 * 60_000).toISOString(),
        package: "wiki-kit",
      },
      {
        ...base,
        id: "native:google:ada@example.com",
        kind: "native",
        label: "Google",
        account: "ada@example.com",
        remote: true,
        status: "needs_repair",
        reason: { key: "sign_in_again" },
        checkedAt: null,
        package: null,
      },
      {
        ...base,
        id: "folder:invoices",
        kind: "folder",
        label: "Invoices",
        account: null,
        path: "/tmp/invoices",
        remote: false,
        status: "not_checked",
        reason: { key: "not_checked" },
        checkedAt: null,
        package: null,
      },
    ];
    const handlers: Record<string, (args: unknown) => unknown> = {
      get_onboarding_state: () => ({ completed: true, skipped: false, started_at: null }),
      read_config_file: () => ({ path: "~/.apollia/apollia.toml", exists: true, text: "", sha256: "", diagnostics: [], locked: [], pendingRestart: [] }),
      get_system_info: () => ({}),
      governance_list_tools: () => [],
      list_connections: () => connections,
      list_mcp_servers: () => {
        throw "runtime unreachable";
      },
    };
    let nextCallbackId = 1;
    (globalThis as unknown as { __TAURI_INTERNALS__?: TauriInternalsStub }).__TAURI_INTERNALS__ = {
      invoke: (cmd: string, args: unknown) => {
        const fn = handlers[cmd];
        if (fn) {
          try {
            return Promise.resolve(fn(args));
          } catch (err) {
            return Promise.reject(err);
          }
        }
        if (cmd.startsWith("list_") || cmd.startsWith("oauth_")) return Promise.resolve([]);
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

const ROWS = '[data-testid^="sidebar-mcp-"], [data-testid^="sidebar-connection-"]';

test.beforeEach(async ({ page }) => {
  await installTauriStub(page);
  await page.goto("/");
  await page.getByTestId("nav-integrations").first().click();
});

test("what needs repair leads the list, and a failed source is one banner", async ({ page }) => {
  // GIVEN the three connections, the state of the services unreadable
  const list = page.getByTestId("connections-sidebar-list");

  // WHEN the page is shown
  await expect(list.locator(ROWS)).toHaveCount(3);

  // THEN the account to repair comes first, then the rest by name: the folder
  // never checked, neutral, then the working service, brought by its package;
  // the failed source has its banner and the list stays
  const ids = await list.locator(ROWS).evaluateAll((rows) => rows.map((r) => r.getAttribute("data-testid")));
  expect(ids).toEqual([
    "sidebar-connection-native:google:ada@example.com",
    "sidebar-connection-folder:invoices",
    "sidebar-mcp-notion",
  ]);
  await expect(page.getByTestId("connection-state-folder:invoices")).toHaveAttribute("data-status", "not_checked");
  await expect(page.getByTestId("connection-package-mcp:notion")).toBeVisible();
  await expect(page.getByTestId("connections-source-error-services")).toBeVisible();
  await expect(page.getByTestId("connections-source-error-connections")).toHaveCount(0);
});

test("the summary counts the connections and filters the grid on a state", async ({ page }) => {
  // GIVEN the three connections, one working
  const list = page.getByTestId("connections-sidebar-list");
  await expect(list.locator(ROWS)).toHaveCount(3);

  // WHEN the summary is read, then Needs attention is chosen
  await expect(page.getByTestId("connections-summary-all")).toHaveAttribute("data-count", "3");
  await expect(page.getByTestId("connections-summary-working")).toHaveAttribute("data-count", "1");
  await expect(page.getByTestId("connections-summary-action")).toHaveAttribute("data-count", "2");
  await page.getByTestId("connections-summary-action").click();

  // THEN only the account to repair and the folder never checked are left, the
  // service and the built-in tools are hidden, and the count is pressed
  await expect(list.locator(ROWS)).toHaveCount(2);
  await expect(page.getByTestId("sidebar-mcp-notion")).toHaveCount(0);
  await expect(page.getByTestId("connections-builtin-section")).toHaveCount(0);
  await expect(page.getByTestId("connections-summary-action")).toHaveAttribute("aria-pressed", "true");

  // AND choosing it again gives every state back
  await page.getByTestId("connections-summary-action").click();
  await expect(list.locator(ROWS)).toHaveCount(3);
});

test("the tiles lay out in columns across the width, and in one column beside an open card", async ({ page }) => {
  // GIVEN the three tiles, nothing open
  const tiles = page.getByTestId("connections-sidebar-list").locator(ROWS);
  await expect(tiles).toHaveCount(3);
  const tops = async (): Promise<number[]> =>
    tiles.evaluateAll((els) => els.map((el) => Math.round(el.getBoundingClientRect().top)));

  // WHEN nothing is open
  // THEN the three tiles share one line
  expect(new Set(await tops()).size).toBe(1);

  // AND once a card is open, they stack in the column beside it
  await page.getByTestId("sidebar-mcp-notion").click();
  await expect(page.getByTestId("connector-detail")).toBeVisible();
  expect(new Set(await tops()).size).toBe(3);
});

test("the card opens beside the list, and the page keeps its place across screens", async ({ page }) => {
  // GIVEN the folders chip and the folder's card open
  // (by its name: the middle of a tile never checked is its Check button)
  await page.getByTestId("connections-kind-filter-folder").click();
  await page.getByTestId("connection-tile-name-folder:invoices").click();
  await expect(page.getByTestId("connector-detail")).toBeVisible();
  await expect(page.getByTestId("sidebar-connection-folder:invoices")).toBeVisible();

  // WHEN the user leaves for another screen and comes back
  await page.getByTestId("nav-library").first().click();
  await expect(page.getByTestId("connections-route")).toHaveCount(0);
  await page.getByTestId("nav-integrations").first().click();

  // THEN the card and the chip are as they were
  await expect(page.getByTestId("connector-detail")).toBeVisible();
  await expect(page.getByTestId("connection-detail-state")).toBeVisible();
  await expect(page.getByTestId("connections-sidebar-list").locator(ROWS)).toHaveCount(1);

  // AND closing the card keeps the list
  await page.getByTestId("connections-back").click();
  await expect(page.getByTestId("connector-detail")).toHaveCount(0);
  await expect(page.getByTestId("sidebar-connection-folder:invoices")).toBeVisible();
});
