import { test, expect, type Page } from "@playwright/test";

/**
 * Adding a connection of the selection, through the one flow of an addition:
 * choose it, read its card and its warning, add it, say who may use it.
 *
 * Boots the UI bundle with the Tauri bridge stubbed (the three members the
 * boot path reaches for, as in `tests/agents/agent-sheet.spec.ts`), reaches
 * the Connections screen through its sidebar entry and opens its add window.
 * From there the clicks are counted on the document: one on the entry, one on
 * Add in its card, which accepts the warning it shows, one on Continue once
 * the agents are checked, and the connection is added, granted and tried with
 * no other gesture. Cancelling gives the search back, as it was typed. The
 * stub answers a remote entry that needs no authentication, an addition that
 * succeeds, the connection and its two tools, and a live trial; a second case
 * answers the organisation's refusal of online connections.
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
    const grants: unknown[] = [];
    (globalThis as unknown as { __invokeCalls: string[] }).__invokeCalls = calls;
    (globalThis as unknown as { __grants: unknown[] }).__grants = grants;
    let added = false;
    const connection = {
      id: "mcp:com-notion-mcp", kind: "mcp", label: "com-notion-mcp", account: null,
      provenance: "selection", remote: true, official: true, state: "connected",
      status: "working", reason: null, checkedAt: null, package: null,
      tools: [
        { id: "mcp:com-notion-mcp/search", family: "read", familySource: "server", prechecked: true },
        { id: "mcp:com-notion-mcp/update", family: "write_or_delete", familySource: "server", prechecked: false },
      ],
    };
    const handlers: Record<string, (args: unknown) => unknown> = {
      get_onboarding_state: () => ({ completed: true, skipped: false, started_at: null }),
      read_config_file: () => ({ path: "~/.apollia/apollia.toml", exists: true, text: "", sha256: "", diagnostics: [], locked: [], pendingRestart: [] }),
      get_system_info: () => ({}),
      fetch_mcp_curated: () => [notion],
      // Connections lists the built-in tools as the Apollia source; null would break the count.
      governance_list_tools: () => [],
      // Asked by "Explore all" only; opening the catalogue never calls it.
      fetch_mcp_registry: () => [notion],
      test_mcp_connection: () => ({
        kind: "success", server_info: "notion", protocol_version: "2025-06-18",
        tools: [], test_duration_ms: 5, live_health: null,
      }),
      add_mcp_server: (args) => {
        if (refuseAddition) {
          throw "refused by the organisation policy: connections to online services are not allowed";
        }
        added = true;
        const name = (args as { config: { name: string } }).config.name;
        return { name, health: { state: "healthy", verified: true } };
      },
      list_connections: () => (added ? [connection] : []),
      list_agents: () => [{ name: "analyst", agent_type: "worker" }],
      grant_connection: (args) => {
        grants.push(args);
        return { connection: "mcp:com-notion-mcp", agents: [], sharedWith: [] };
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

/** Open the add window of Connections, then count every click from there on. */
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

async function invokeCalls(page: Page): Promise<string[]> {
  return page.evaluate(() => (globalThis as unknown as { __invokeCalls: string[] }).__invokeCalls);
}

test.describe("the public catalogue is asked on request only", () => {
  test("opening the selection asks nothing, Explore all asks once", async ({ page }) => {
    // GIVEN the selection open
    await installTauriStub(page, false);
    await openSelection(page);

    // WHEN nothing else is done, then Explore all is pressed
    const beforeExplore = await invokeCalls(page);
    await page.getByTestId("catalogue-explore-all").click();
    await expect(page.getByTestId("catalogue-back-to-selection")).toBeVisible();

    // THEN the registry was asked only after the button, and once
    expect(beforeExplore).not.toContain("fetch_mcp_registry");
    expect((await invokeCalls(page)).filter((c) => c === "fetch_mcp_registry")).toHaveLength(1);
  });
});

test.describe("adding a connection", () => {
  test("three clicks from the window add the connection, give it to Apollia and try it", async ({ page }) => {
    // GIVEN the add window open on an entry that needs no authentication
    await installTauriStub(page, false);
    await openSelection(page);

    // WHEN the entry is chosen, Add is pressed on its card under its warning
    await page.getByTestId("catalogue-entry-com.notion/mcp").click();
    await expect(page.getByTestId("catalogue-card-needs")).toBeVisible();
    await expect(page.getByTestId("wizard-step-disclaimer")).toBeVisible();
    await page.getByTestId("catalogue-card-add").click();

    // THEN the flow asks who may use it: Apollia checked, the other agent not
    const who = page.getByTestId("add-flow-who");
    await expect(who).toBeVisible();
    await expect(page.getByTestId("add-flow-steps")).toHaveAttribute("data-step", "who");
    const apollia = who.locator('[data-agent="apollia:chat"] [role="checkbox"]');
    const analyst = who.locator('[data-agent="analyst"] [role="checkbox"]');
    await expect(apollia).toHaveAttribute("aria-checked", "true");
    await expect(analyst).toHaveAttribute("aria-checked", "false");

    // WHEN Continue is pressed
    await page.getByTestId("add-flow-who-continue").click();

    // THEN Apollia is given every tool of the connection, and the trial succeeds
    await expect(page.getByTestId("test-result-success")).toBeVisible();
    await expect(page.getByTestId("add-flow-steps")).toHaveAttribute("data-step", "trial");
    expect(await clicks(page)).toBe(3);
    const calls = await invokeCalls(page);
    expect(calls.indexOf("add_mcp_server")).toBeLessThan(calls.indexOf("grant_connection"));
    expect(calls.indexOf("grant_connection")).toBeLessThan(calls.indexOf("test_mcp_live_server"));
    const grants = await page.evaluate(() => (globalThis as unknown as { __grants: unknown[] }).__grants);
    expect(grants).toEqual([
      {
        connectionId: "mcp:com-notion-mcp",
        agents: ["apollia:chat"],
        tools: ["mcp:com-notion-mcp/search", "mcp:com-notion-mcp/update"],
      },
    ]);
  });

  test("another agent checked gets the connection in the same grant", async ({ page }) => {
    // GIVEN the flow of the entry on who may use it
    await installTauriStub(page, false);
    await openSelection(page);
    await page.getByTestId("catalogue-entry-com.notion/mcp").click();
    await page.getByTestId("catalogue-card-add").click();
    await expect(page.getByTestId("add-flow-who")).toBeVisible();

    // WHEN the other agent is checked too, then Continue pressed
    await page.locator('[data-agent="analyst"] [role="checkbox"]').click();
    await page.getByTestId("add-flow-who-continue").click();

    // THEN one grant names both agents
    await expect(page.getByTestId("test-result-success")).toBeVisible();
    const grants = await page.evaluate(() => (globalThis as unknown as { __grants: Array<{ agents: string[] }> }).__grants);
    expect(grants).toHaveLength(1);
    expect(grants[0].agents).toEqual(["apollia:chat", "analyst"]);
  });

  test("a refusal by the organisation says who refused, in the same two clicks", async ({ page }) => {
    // GIVEN an organisation policy that refuses online connections
    await installTauriStub(page, true);
    await openSelection(page);

    // WHEN the entry is chosen, then added from its card
    await page.getByTestId("catalogue-entry-com.notion/mcp").click();
    await page.getByTestId("catalogue-card-add").click();

    // THEN the one refusal box shows, with its action, and nothing was granted
    const refusal = page.getByTestId("connection-refusal");
    await expect(refusal).toBeVisible();
    await expect(refusal).toHaveAttribute("data-refusal", "remote_denied");
    await expect(page.getByTestId("connection-refusal-open")).toBeVisible();
    expect(await clicks(page)).toBe(2);
    expect(await invokeCalls(page)).not.toContain("grant_connection");
  });

  test("cancelling gives the search back, not the list", async ({ page }) => {
    // GIVEN a search typed in the add window, and the card of its entry open
    await installTauriStub(page, false);
    await openSelection(page);
    await page.getByTestId("catalogue-search").fill("notion");
    await page.getByTestId("catalogue-entry-com.notion/mcp").click();
    await expect(page.getByTestId("wizard-step-disclaimer")).toBeVisible();

    // WHEN the card is cancelled, then the flow of an address opened and cancelled
    await page.getByTestId("catalogue-card-cancel").click();
    await expect(page.getByTestId("catalogue-search")).toHaveValue("notion");
    await page.getByTestId("catalogue-search").fill("");
    await page.getByTestId("catalogue-address").click();
    await expect(page.getByTestId("add-flow-address")).toBeVisible();
    await page.getByTestId("wizard-cancel-btn").click();

    // THEN the window is back on its search each time, with no card nor flow open
    await expect(page.getByTestId("catalogue-entry-com.notion/mcp")).toBeVisible();
    await expect(page.getByTestId("catalogue-card")).toHaveCount(0);
    await expect(page.getByTestId("connector-wizard")).toHaveCount(0);
  });
});

test.describe("a source that does not answer", () => {
  test("a public catalogue that fails is a banner, the selection stays", async ({ page }) => {
    // GIVEN a public catalogue that cannot be reached
    await installTauriStub(page, false);
    await page.addInitScript(() => {
      const g = globalThis as unknown as { __TAURI_INTERNALS__: { invoke: (c: string, a: unknown) => unknown } };
      const base = g.__TAURI_INTERNALS__.invoke;
      g.__TAURI_INTERNALS__.invoke = (cmd: string, args: unknown) =>
        cmd === "fetch_mcp_registry" ? Promise.reject("the registry did not answer") : base(cmd, args);
    });
    await openSelection(page);

    // WHEN Explore all is pressed
    await page.getByTestId("catalogue-explore-all").click();

    // THEN a banner says so, the selection is still listed, nothing reads as the end of a catalogue
    await expect(page.getByTestId("catalogue-failure")).toBeVisible();
    await expect(page.getByTestId("catalogue-entry-com.notion/mcp")).toBeVisible();
    await expect(page.getByTestId("catalogue-explore-all")).toBeVisible();
  });
});
