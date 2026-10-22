import { test, expect, type Page } from "@playwright/test";

/**
 * The Connections page: one list beside the card of the chosen connection.
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
        reason: { key: "not_checked" },
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
        status: "needs_repair",
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

  // THEN the two never checked come first, by name, then the working service,
  // brought by its package; the failed source has its banner and the list stays
  const ids = await list.locator(ROWS).evaluateAll((rows) => rows.map((r) => r.getAttribute("data-testid")));
  expect(ids).toEqual([
    "sidebar-connection-native:google:ada@example.com",
    "sidebar-connection-folder:invoices",
    "sidebar-mcp-notion",
  ]);
  await expect(page.getByTestId("connection-state-folder:invoices")).toHaveAttribute("data-status", "needs_repair");
  await expect(page.getByTestId("connection-package-mcp:notion")).toBeVisible();
  await expect(page.getByTestId("connections-source-error-services")).toBeVisible();
  await expect(page.getByTestId("connections-source-error-connections")).toHaveCount(0);
});

test("the card opens beside the list, and the page keeps its place across screens", async ({ page }) => {
  // GIVEN the folders chip and the folder's card open
  await page.getByTestId("connections-kind-filter-folder").click();
  await page.getByTestId("sidebar-connection-folder:invoices").click();
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
