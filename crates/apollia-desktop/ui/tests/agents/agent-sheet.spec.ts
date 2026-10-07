import { test, expect, type Page } from "@playwright/test";

/**
 * The sheet of an agent, read without code.
 *
 * Boots the UI bundle with the Tauri bridge stubbed (the three members the
 * boot path reaches for, as in `tests/responsive/multi-viewport.spec.ts`),
 * reaches the Agents screen through its sidebar entry, and reads the sheet of
 * one installed agent and of the built-in Apollia agent. The stub answers the
 * commands the sheet calls with a resolved agent holding a mission, a local
 * model that fell back once, three accesses of three families, the profile as
 * a source, one run, one change and two memory entries, searched by substring.
 */

type InvokeStub = (cmd: string, args: unknown) => unknown;

interface TauriInternalsStub {
  invoke: InvokeStub;
  transformCallback: (cb: (payload: unknown) => void, once: boolean) => number;
  convertFileSrc: (path: string) => string;
}

async function installTauriStub(page: Page): Promise<void> {
  await page.addInitScript(() => {
    const agent = {
      id: "rt-invoices",
      name: "invoices",
      version: "1.2.0",
      enabled: true,
      runtime_status: "active",
      installed_at: "2026-10-01T09:00:00Z",
      description: "Declared description",
      tags: [],
      execution_mode: "direct",
      supports_a2a: false,
      skills: [],
      agent_type: "assistant",
      examples: [],
      limitations: [],
      setup_notes: null,
      class_name: "Invoices",
      memory_namespace: "invoices",
      shared_memory_namespaces: [],
    };
    const descriptors: Record<string, unknown> = {
      file_read: {
        name: "file_read", version: "1", description: "Reads the files of the folder you chose",
        kind: { type: "native" }, input_schema: null, output_schema: null, permissions: [],
        family: "read", family_source: "builtin",
      },
      mail_send: {
        name: "mail_send", version: "1", description: "Sends a mail from your account",
        kind: { type: "mcp_server" }, input_schema: null, output_schema: null, permissions: [],
        family: "send_external", family_source: "selection",
      },
    };
    let memory = [
      { id: "m1", entry_type: "semantic", key: "pref.tone", value: "Prefers short quotes", created_at: "2026-10-02T09:00:00Z", expires_at: null, score: null },
      { id: "m2", entry_type: "episodic", key: "quote.42", value: "Quote 42 sent to Acme", created_at: "2026-10-01T09:00:00Z", expires_at: null, score: null },
    ];
    const handlers: Record<string, (args: unknown) => unknown> = {
      get_onboarding_state: () => ({ completed: true, skipped: false, started_at: null }),
      read_config_file: () => ({ path: "~/.apollia/apollia.toml", exists: true, text: "", sha256: "", diagnostics: [], locked: [], pendingRestart: [] }),
      get_system_info: () => ({}),
      list_agents: () => [agent],
      get_agent_definition: (args) => {
        const name = (args as { name: string }).name;
        if (name === "apollia:chat") {
          return {
            id: "apollia:chat", origin: "interface", package: null, version: "1", enabled: true,
            name: "Apollia", mission: "", examples: [], access: [], sources: ["profile"],
            model: null, autonomy: "supervised", profile: null, overridden: [], definition: {},
          };
        }
        return {
          id: "invoices", origin: "package", package: "billing", version: "1.2.0", enabled: true,
          name: "Invoices", mission: "Sort the invoices of the week", examples: ["Sort this week's invoices"],
          access: ["file_read", "mail_send", "agent:veille"], sources: ["profile"],
          model: { agent: "invoices", model_source: "local", model_id: "qwen3-8b", preset_id: null },
          autonomy: "supervised", profile: null, overridden: ["mission"], definition: {},
        };
      },
      get_agent_model_fallback: (args) =>
        (args as { agent: string }).agent === "invoices"
          ? { agent: "invoices", requested: "qwen3-8b", used: "llama-3", at: "2026-10-02T09:12:00Z" }
          : null,
      list_agent_changes: (args) =>
        (args as { agent: string }).agent === "invoices"
          ? [{
              seq: 0, run_id: "governance:invoices", ts: "2026-10-02T10:00:00Z", kind: "access_changed",
              payload: { event: "access_changed", access_id: "mail_send", change: "granted", actor: "api" },
            }]
          : [],
      describe_tool: (args) => descriptors[(args as { name: string }).name] ?? null,
      list_tasks: () => [{
        id: "task-1", agent_id: "rt-invoices", agent_name: "invoices", status: "completed",
        input_preview: "Sort the invoices of Monday", output_text: "Three invoices sorted",
        duration_ms: 1200, created_at: "2026-10-02T08:00:00Z",
      }],
      get_task_timeline: () => [],
      get_task_trace: () => null,
      list_memory_entries: () => memory,
      search_memory: (args) => {
        const { namespace, query } = args as { namespace: string; query: string };
        if (namespace !== "invoices") return [];
        const needle = query.toLowerCase();
        return memory
          .filter((m) => m.value.toLowerCase().includes(needle))
          .map((m) => ({
            id: m.id, entry_type: m.entry_type, content: m.value, score: 1, relevance: null,
            created_at: m.created_at,
          }));
      },
      delete_memory_entry: (args) => {
        const id = (args as { entryId: string }).entryId;
        memory = memory.filter((m) => m.id !== id);
        return true;
      },
      clear_memory: () => {
        const n = memory.length;
        memory = [];
        return n;
      },
    };
    let nextCallbackId = 1;
    (globalThis as unknown as { __TAURI_INTERNALS__?: TauriInternalsStub }).__TAURI_INTERNALS__ = {
      invoke: (cmd: string, args: unknown) => {
        const fn = handlers[cmd];
        if (fn) return Promise.resolve(fn(args));
        if (cmd.startsWith("list_")) return Promise.resolve([]);
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

async function openAgents(page: Page): Promise<void> {
  await page.goto("/");
  await page.getByTestId("nav-agents").first().click();
  await expect(page.getByTestId("agents-page")).toBeVisible();
}

test.describe("agent sheet", () => {
  test.beforeEach(async ({ page }) => {
    await installTauriStub(page);
  });

  test("an installed agent reads mission, model, access, sources, activity and memory in that order", async ({ page }) => {
    // GIVEN the Agents screen with one installed agent
    await openAgents(page);

    // WHEN the agent is selected
    await page.getByTestId("agent-list-row").first().click();
    const sheet = page.getByTestId("agent-sheet");
    await expect(sheet).toBeVisible();

    // THEN the six sections come in the order of the sheet
    const order = await sheet.locator(":scope > section").evaluateAll((nodes) =>
      nodes.map((n) => n.getAttribute("data-testid")),
    );
    expect(order).toEqual([
      "agent-sheet-mission",
      "agent-sheet-model",
      "agent-sheet-access",
      "agent-sheet-sources",
      "agent-sheet-activity",
      "agent-sheet-memory",
    ]);
    // AND the mission is the resolved one, the model the assigned one with its fallback notice
    await expect(page.getByTestId("agent-sheet-mission-text")).toHaveText("Sort the invoices of the week");
    await expect(page.getByTestId("agent-sheet-model-name")).toHaveText("qwen3-8b");
    await expect(page.getByTestId("agent-sheet-model-fallback")).toContainText("llama-3");
    // AND the accesses are grouped by family with what each permits, no raw identifier shown
    const read = page.locator('[data-testid="agent-sheet-access-group"][data-family="read"]');
    await expect(read).toContainText("Reads the files of the folder you chose");
    const send = page.locator('[data-testid="agent-sheet-access-group"][data-family="send_external"]');
    await expect(send).toContainText("Sends a mail from your account");
    await expect(page.getByTestId("agent-sheet-access-id")).toHaveCount(0);
    await expect(page.getByTestId("agent-sheet-access")).not.toContainText("file_read");
    // AND the profile is listed among the sources
    await expect(page.locator('[data-testid="agent-sheet-source"][data-source="profile"]')).toBeVisible();
    // AND the change of access reads in plain words
    await expect(page.getByTestId("agent-sheet-change")).toContainText("Sends a mail from your account");
  });

  test("a run opens on the sheet with its input, status, output and steps", async ({ page }) => {
    // GIVEN the sheet of the installed agent
    await openAgents(page);
    await page.getByTestId("agent-list-row").first().click();

    // WHEN its run is opened
    await page.getByTestId("agent-sheet-run-open").first().click();

    // THEN input, status, output and steps show, and no technical detail outside developer mode
    const detail = page.getByTestId("agent-sheet-run-detail");
    await expect(detail.getByTestId("agent-sheet-run-input")).toContainText("Sort the invoices of Monday");
    await expect(detail.getByTestId("agent-sheet-run-status")).toBeVisible();
    await expect(detail.getByTestId("agent-sheet-run-output")).toContainText("Three invoices sorted");
    await expect(detail.getByTestId("agent-sheet-run-trace")).toBeVisible();
    await expect(page.getByTestId("agent-sheet-run-technical")).toHaveCount(0);
  });

  test("the memory is read, one entry deleted, then all erased after a confirmation", async ({ page }) => {
    // GIVEN the sheet of an agent that retained two entries
    await openAgents(page);
    await page.getByTestId("agent-list-row").first().click();
    await expect(page.getByTestId("agent-memory-row")).toHaveCount(2);

    // WHEN one entry is deleted
    await page.getByTestId("agent-sheet-memory-delete").first().click();

    // THEN one entry remains
    await expect(page.getByTestId("agent-memory-row")).toHaveCount(1);

    // WHEN everything is erased and the confirmation accepted
    await page.getByTestId("agent-sheet-memory-clear").click();
    await page.getByTestId("agent-sheet-memory-clear-dialog-confirm").click();

    // THEN the memory is empty
    await expect(page.getByTestId("agent-sheet-memory-empty")).toBeVisible();
  });

  test("the memory is searched on the sheet, a miss said in words, the full list back once the search is emptied", async ({ page }) => {
    // GIVEN the sheet of an agent that retained two entries
    await openAgents(page);
    await page.getByTestId("agent-list-row").first().click();
    await expect(page.getByTestId("agent-memory-row")).toHaveCount(2);

    // WHEN a word of one entry is searched
    await page.getByTestId("agent-sheet-memory-search").fill("Acme");

    // THEN only that entry remains, counted against the whole memory
    await expect(page.getByTestId("agent-memory-row")).toHaveCount(1);
    await expect(page.getByTestId("agent-memory-row")).toContainText("Quote 42 sent to Acme");
    await expect(page.getByTestId("agent-sheet-memory-count")).toContainText("2");

    // WHEN a word no entry holds is searched
    await page.getByTestId("agent-sheet-memory-search").fill("zeppelin");

    // THEN no row shows and the empty state says nothing matched
    await expect(page.getByTestId("agent-memory-row")).toHaveCount(0);
    await expect(page.getByTestId("agent-sheet-memory-empty")).toBeVisible();

    // WHEN the search is emptied
    await page.getByTestId("agent-sheet-memory-search").fill("");

    // THEN both entries are listed again
    await expect(page.getByTestId("agent-memory-row")).toHaveCount(2);
  });

  test("the Apollia agent has the same sheet", async ({ page }) => {
    // GIVEN the Agents screen
    await openAgents(page);

    // WHEN the built-in Apollia agent is selected
    await page.getByTestId("apollia-chat-pinned").click();

    // THEN its sheet carries the same sections, its model following the default
    const sheet = page.getByTestId("agent-sheet");
    await expect(sheet).toHaveAttribute("data-agent", "apollia:chat");
    for (const section of ["mission", "model", "access", "sources", "activity", "memory"]) {
      await expect(page.getByTestId(`agent-sheet-${section}`)).toBeVisible();
    }
    await expect(page.locator('[data-testid="agent-sheet-source"][data-source="profile"]')).toBeVisible();
  });
});
