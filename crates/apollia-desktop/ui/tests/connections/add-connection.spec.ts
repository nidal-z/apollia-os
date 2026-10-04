import { test, expect, type Page } from "@playwright/test";

/**
 * Adding a connection of the selection: choose it, accept its warning.
 *
 * Boots the UI bundle with the Tauri bridge stubbed (the three members the
 * boot path reaches for, as in `tests/agents/agent-sheet.spec.ts`), reaches
 * the Connections screen through its sidebar entry and opens its selection.
 * From there the clicks are counted on the document: one on the entry, one on
 * the warning's button, and the connection is added and tried with no other
 * gesture. The stub answers a remote entry that needs no authentication, an
 * addition that succeeds and a live trial of two tools; a second case answers
 * the organisation's refusal of online connections.
 */

type InvokeStub = (cmd: string, args: unknown) => unknown;

interface TauriInternalsStub {
  invoke: InvokeStub;
  transformCallback: (cb: (payload: unknown) => void, once: boolean) => number;
  convertFileSrc: (path: string) => string;
}

async function installTauriStub(page: Page, refuse: boolean): Promise<void> {
  await page.addInitScript((refuseAddition: boolean) => {
    const notion = {
      name: "com.notion/mcp",
      title: "Notion",
      description: "Pages and databases",
      version: "1.0.0",
      repository_url: null,
      website_url: null,
      packages: null,
      trust_level: "official",
      category: "productivity",
      enrichment: null,
      is_installed: false,
      remotes: [{ type: "streamable-http", url: "https://mcp.notion.com/mcp", headers: [] }],
    };
    const calls: string[] = [];
    (globalThis as unknown as { __invokeCalls: string[] }).__invokeCalls = calls;
    const handlers: Record<string, (args: unknown) => unknown> = {
      get_onboarding_state: () => ({ completed: true, skipped: false, started_at: null }),
      get_config: () => ({ config_path: "~/.apollia/apollia.toml", config_exists: true, sections: [] }),
      get_system_info: () => ({}),
      fetch_mcp_curated: () => [notion],
      // Connections lists the built-in tools as the Apollia source; null would break the count.
      governance_list_tools: () => [],
      // Opening the catalogue still chains the full registry, which keeps the selection.
      fetch_mcp_registry: () => [notion],
      test_mcp_connection: () => ({
        kind: "success", server_info: "notion", protocol_version: "2025-06-18",
        tools: [], test_duration_ms: 5, live_health: null,
      }),
      add_mcp_server: (args) => {
        if (refuseAddition) {
          throw "refused by the organisation policy: connections to online services are not allowed";
        }
        const name = (args as { config: { name: string } }).config.name;
        return { name, health: { state: "healthy", verified: true } };
      },
      test_mcp_live_server: () => ({
        kind: "success", server_info: "notion", protocol_version: "2025-06-18",
        tools: [{ name: "search" }, { name: "fetch" }], test_duration_ms: 9,
        live_health: { state: "healthy", verified: true },
      }),
    };
    let nextCallbackId = 1;
    (globalThis as unknown as { __TAURI_INTERNALS__?: TauriInternalsStub }).__TAURI_INTERNALS__ = {
      invoke: (cmd: string, args: unknown) => {
        calls.push(cmd);
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
  }, refuse);
}

/** Open the selection of Connections, then count every click from there on. */
async function openSelection(page: Page): Promise<void> {
  await page.goto("/");
  await page.getByTestId("nav-integrations").first().click();
  await page.getByTestId("connections-open-catalogue").click();
  await expect(page.getByTestId("catalogue-entry-com.notion/mcp")).toBeVisible();
  await page.evaluate(() => {
    const g = globalThis as unknown as { __clicks: number };
    g.__clicks = 0;
    document.addEventListener("click", () => (g.__clicks += 1), true);
  });
}

async function clicks(page: Page): Promise<number> {
  return page.evaluate(() => (globalThis as unknown as { __clicks: number }).__clicks);
}

test.describe("adding a connection", () => {
  test("two clicks from the selection add the connection and try it", async ({ page }) => {
    // GIVEN the selection open on an entry that needs no authentication
    await installTauriStub(page, false);
    await openSelection(page);

    // WHEN the entry is chosen, then its warning accepted
    await page.getByTestId("catalogue-entry-com.notion/mcp").click();
    await expect(page.getByTestId("wizard-step-disclaimer")).toBeVisible();
    await expect(page.locator('[data-testid="connector-wizard"] input[type="checkbox"]')).toHaveCount(0);
    await page.getByTestId("wizard-warning-accept").click();

    // THEN the connection is added and its trial succeeds, with no other gesture
    await expect(page.getByTestId("test-result-success")).toBeVisible();
    expect(await clicks(page)).toBe(2);
    const calls = await page.evaluate(
      () => (globalThis as unknown as { __invokeCalls: string[] }).__invokeCalls,
    );
    expect(calls.indexOf("add_mcp_server")).toBeLessThan(calls.indexOf("test_mcp_live_server"));
  });

  test("a refusal by the organisation says who refused, in the same two clicks", async ({ page }) => {
    // GIVEN an organisation policy that refuses online connections
    await installTauriStub(page, true);
    await openSelection(page);

    // WHEN the entry is chosen, then its warning accepted
    await page.getByTestId("catalogue-entry-com.notion/mcp").click();
    await page.getByTestId("wizard-warning-accept").click();

    // THEN the one refusal box shows, with its action
    const refusal = page.getByTestId("connection-refusal");
    await expect(refusal).toBeVisible();
    await expect(refusal).toHaveAttribute("data-refusal", "remote_denied");
    await expect(page.getByTestId("connection-refusal-open")).toBeVisible();
    expect(await clicks(page)).toBe(2);
  });
});
