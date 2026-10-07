import { test, expect, type Page } from "@playwright/test";

/**
 * A batch is followed in its view: the table fills as the items end, the
 * batch is paused, resumed, stopped, retried and exported from there, a
 * waiting item opens its decision, the end is announced by a system
 * notification, and the history opens a past batch.
 *
 * Boots the UI bundle with the Tauri bridge stubbed. The batch engine is a
 * small model held in the page: a batch launched on the test source, a CSV
 * file of five rows, whose items the test ends one by one before emitting the
 * runtime event the real bridge emits. The stub keeps every call.
 */

interface Call {
  cmd: string;
  args: Record<string, unknown>;
}

interface StubItem {
  rank: number;
  origin: string;
  input: Record<string, string>;
  status: string;
  output: string | null;
  sources: unknown;
  taskId: string | null;
}

interface Engine {
  state: string;
  items: StubItem[];
  endItem: (rank: number, status: string, output: string) => void;
  emit: (category: string, eventType: string, inner: Record<string, unknown>) => void;
}

type Globals = {
  __batchCalls: Call[];
  __engine: Engine;
  __notifications: { title: string; body: string }[];
};

async function installTauriStub(page: Page): Promise<void> {
  await page.addInitScript(() => {
    const agent = {
      id: "rt-triage", name: "triage", version: "1.0.0", enabled: true, runtime_status: "active",
      installed_at: "2026-10-01T09:00:00Z", description: "Qualifies the requests", tags: [],
      execution_mode: "direct",
      supports_a2a: false, skills: [], agent_type: "assistant", examples: [], limitations: [],
      setup_notes: null, class_name: "Triage", memory_namespace: "triage", shared_memory_namespaces: [],
    };
    const calls: Call[] = [];
    const notifications: { title: string; body: string }[] = [];
    const listeners: number[] = [];
    const g = globalThis as unknown as Globals & Record<string, unknown>;
    g.__batchCalls = calls;
    g.__notifications = notifications;

    // The system notifications the plugin sends go through this class.
    class NotificationStub {
      static permission = "granted";
      static requestPermission = () => Promise.resolve("granted");
      constructor(title: string, options?: { body?: string }) {
        notifications.push({ title, body: options?.body ?? "" });
      }
    }
    g.Notification = NotificationStub;

    const clients = ["Acme", "Bonnet", "Lumen", "Mercier", "Petit"];
    const engine: Engine = {
      state: "running",
      items: clients.map((client, rank) => ({
        rank, origin: `row ${rank + 2}`, input: { client, request: `Request of ${client}` },
        status: "pending", output: null, sources: null, taskId: null,
      })),
      endItem(rank, status, output) {
        const item = engine.items[rank];
        item.status = status;
        item.output = output;
        item.taskId = `task-${rank}`;
        item.sources = status === "completed" ? [{ title: `Sheet of ${item.input.client}` }] : null;
      },
      emit(category, eventType, inner) {
        const event = { event: "runtime-event", id: 1, payload: { category, event_type: eventType, payload: { [eventType]: inner } } };
        for (const id of listeners) (g[`_${id}`] as ((e: unknown) => void) | undefined)?.(event);
      },
    };
    g.__engine = engine;

    const count = (status: string) => engine.items.filter((item) => item.status === status).length;
    const view = (batchId: string) => ({
      batchId, agent: "triage",
      source: { kind: "csv", location: "/data/test-source.csv", options: {}, write: { destination: "copy", columns: ["Category"] } },
      grants: ["send_email"], concurrency: 1, maxFailures: 10, state: engine.state,
      total: engine.items.length, completed: count("completed"), failed: count("failed"),
      canceled: count("canceled"), awaiting: count("awaiting"),
      createdAt: "2026-10-05T09:00:00Z", updatedAt: "2026-10-05T09:04:00Z",
      endedAt: engine.state === "running" || engine.state === "paused" ? null : "2026-10-05T09:06:00Z",
    });
    const past = (index: number) => ({
      ...view(`past-${index}`), state: index % 3 === 0 ? "interrupted" : "completed",
      total: 10, completed: 8, failed: index % 2, canceled: 0, awaiting: 0,
      createdAt: new Date(Date.UTC(2026, 8, 30 - (index % 28), 9, index % 60)).toISOString(),
      endedAt: "2026-09-30T10:00:00Z",
    });

    const handlers: Record<string, (args: Record<string, unknown>) => unknown> = {
      get_onboarding_state: () => ({ completed: true, skipped: false, started_at: null }),
      read_config_file: () => ({ path: "~/.apollia/apollia.toml", exists: true, text: "", sha256: "", diagnostics: [], locked: [], pendingRestart: [] }),
      get_system_info: () => ({}),
      list_agents: () => [agent],
      list_batch_sources: () => [{ id: "s-1", kind: "file", label: "Test source", reference: "/data/test-source.csv" }],
      list_batch_accesses: () => [{ accessId: "send_email", permits: "Send an email", family: "send_external", grantable: true }],
      launch_batch: () => ({ batchId: "b-1", agent: "triage", state: "running", total: 5, maxFailures: 10 }),
      get_batch: (args) => (args.batchId === "gone" ? null : { batch: view(String(args.batchId)), items: engine.items }),
      list_batches: (args) => [view("b-1"), ...Array.from({ length: 119 }, (_, index) => past(index))].slice(0, Number(args.limit)),
      control_batch: (args) => {
        const action = String(args.action);
        if (action === "pause") engine.state = "paused";
        if (action === "resume") engine.state = "running";
        if (action === "stop") {
          engine.state = "stopped";
          for (const item of engine.items) if (item.status === "pending" || item.status === "running") item.status = "canceled";
        }
        if (action === "retry") {
          if (count("failed") + count("awaiting") === 0) throw "batch 'b-1' has no failed or waiting item to run again";
          for (const item of engine.items) if (item.status === "failed" || item.status === "awaiting") item.status = "pending";
          if (engine.state !== "paused") engine.state = "running";
        }
        return view(String(args.batchId));
      },
      export_batch: (args) => ({ path: args.path, bytes: 120 }),
      "plugin:dialog|save": (args) => `/exports/${String((args.options as { defaultPath: string }).defaultPath)}`,
      batch_item_decision: (args) => {
        const item = engine.items[Number(args.rank)];
        if (item.status !== "awaiting") return null;
        return {
          id: `batch:${String(args.batchId)}:${item.rank}`, kind: "batch_item", summary: "The agent asks to send an email",
          access: "send_email", answers: ["once", "always", "refuse"], origin: { batchId: args.batchId, rank: item.rank },
        };
      },
      decide_batch_item: (args) => {
        const rank = Number(String(args.id).split(":").at(-1));
        engine.items[rank].status = args.answer === "refuse" ? "failed" : "pending";
        return { id: args.id, answer: args.answer, resumed: 1, message: "ok" };
      },
      get_task_timeline: () => [
        { type: "tool_call", tool_name: "read_page", duration_ms: 20, exit_code: 0, truncated: false, timestamp: "2026-10-05T09:01:00Z" },
        { type: "llm_call", backend: "local", model: "m", prompt_tokens: 1, completion_tokens: 1, cost_usd: null, latency_ms: 1, timestamp: "2026-10-05T09:01:01Z" },
      ],
      "plugin:event|listen": (args) => {
        if (args.event === "runtime-event") listeners.push(Number(args.handler));
        return Number(args.handler);
      },
    };
    let nextCallbackId = 1;
    g.__TAURI_INTERNALS__ = {
      invoke: (cmd: string, args: unknown) => {
        calls.push({ cmd, args: (args ?? {}) as Record<string, unknown> });
        const fn = handlers[cmd];
        if (fn) {
          try {
            return Promise.resolve(fn((args ?? {}) as Record<string, unknown>));
          } catch (refusal) {
            return Promise.reject(refusal);
          }
        }
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

function callsOf(page: Page, cmd: string): Promise<Record<string, unknown>[]> {
  return page.evaluate(
    (name) => (globalThis as unknown as Globals).__batchCalls.filter((call) => call.cmd === name).map((call) => call.args),
    cmd,
  );
}

/** Ends an item in the model, then emits the event the bridge emits for a run that ended. */
async function endItem(page: Page, rank: number, status: string, output: string): Promise<void> {
  await page.evaluate(
    ([r, s, o]) => {
      const engine = (globalThis as unknown as Globals).__engine;
      engine.endItem(r as number, s as string, o as string);
      engine.emit("task-changed", "TaskCompleted", { task_id: `task-${r}` });
    },
    [rank, status, output] as const,
  );
}

/** Launches a batch on the test source from the sheet of the agent. */
async function launchFromSheet(page: Page) {
  await page.goto("/");
  await page.getByTestId("nav-agents").first().click();
  await page.getByTestId("agent-list-row").first().click();
  await page.getByTestId("agent-batch-btn").click();
  const dialog = page.getByTestId("batch-launch-dialog");
  await dialog.getByTestId("batch-launch-family-table").getByRole("radio").click();
  await dialog.getByTestId("batch-launch-source-option").getByRole("radio").click();
  await dialog.getByTestId("batch-launch-confirm").click();
  const view = page.getByTestId("batch-page");
  await expect(view).toBeVisible();
  return view;
}

test.describe("following a batch", () => {
  test.beforeEach(async ({ page }) => {
    await installTauriStub(page);
  });

  test("the table fills row by row, and the progress counts the items dealt with", async ({ page }) => {
    // GIVEN a batch launched on the test source
    const view = await launchFromSheet(page);

    // THEN its view opens under Agents: one row per item, none dealt with yet
    await expect(page.getByTestId("nav-agents").first()).toHaveAttribute("aria-current", "page");
    await expect(view.getByTestId("batch-item")).toHaveCount(5);
    await expect(view.getByTestId("batch-state")).toHaveAttribute("data-state", "running");
    await expect(view.getByTestId("batch-progress")).toHaveAttribute("data-treated", "0");
    await expect(view.getByTestId("batch-item").first()).toContainText("Acme");
    await expect(view.getByTestId("batch-item").first().getByTestId("batch-item-sources")).toContainText("row 2");

    // WHEN the first item succeeds and the second fails
    await endItem(page, 0, "completed", "Quote, high priority");
    await endItem(page, 1, "failed", "The attached file is an image");

    // THEN their rows change at once, with output, cause and cited sources
    const first = view.getByTestId("batch-item").nth(0);
    await expect(first).toHaveAttribute("data-status", "completed");
    await expect(first.getByTestId("batch-item-output")).toHaveText("Quote, high priority");
    await expect(first.getByTestId("batch-item-sources")).toContainText("Sheet of Acme");
    await expect(view.getByTestId("batch-item").nth(1)).toHaveAttribute("data-status", "failed");
    await expect(view.getByTestId("batch-item").nth(1).getByTestId("batch-item-output")).toHaveText("The attached file is an image");

    // AND the progress is two items out of five, not a figure tied to the state
    await expect(view.getByTestId("batch-progress")).toHaveAttribute("data-treated", "2");
    await expect(view.getByTestId("batch-progress")).toHaveAttribute("data-total", "5");
    await expect(view.getByTestId("batch-treated")).toContainText("2");
    await expect(view.getByTestId("batch-count-todo")).toContainText("3");

    // WHEN the failures tab is opened, then the table searched
    await view.getByTestId("batch-tab-failed").click();
    await expect(view.getByTestId("batch-item")).toHaveCount(1);
    await view.getByTestId("batch-tab-all").click();
    await view.getByTestId("batch-search").fill("lumen");

    // THEN only the matching row stays
    await expect(view.getByTestId("batch-item")).toHaveCount(1);
    await expect(view.getByTestId("batch-item")).toContainText("Lumen");
  });

  test("the batch is paused, resumed, retried, exported and stopped from its view", async ({ page }) => {
    // GIVEN a running batch with one success and one failure
    const view = await launchFromSheet(page);
    await endItem(page, 0, "completed", "Quote");
    await endItem(page, 1, "failed", "No address");
    await expect(view.getByTestId("batch-count-failed")).toBeVisible();

    // THEN retry is shown but inert while the batch runs
    await expect(view.getByTestId("batch-retry")).toBeDisabled();

    // WHEN it is paused
    await view.getByTestId("batch-pause").click();

    // THEN it shows paused, resume leads, and its failures can be retried
    await expect(view.getByTestId("batch-state")).toHaveAttribute("data-state", "paused");
    await expect(view.getByTestId("batch-message")).toBeVisible();
    await expect(view.getByTestId("batch-pause")).toHaveCount(0);
    await view.getByTestId("batch-retry").click();
    await expect(view.getByTestId("batch-item").nth(1)).toHaveAttribute("data-status", "pending");

    // WHEN it is resumed, then exported in each format
    await view.getByTestId("batch-resume").click();
    await expect(view.getByTestId("batch-state")).toHaveAttribute("data-state", "running");
    for (const format of ["xlsx", "csv", "json"]) {
      await view.getByTestId("batch-export").click();
      await page.getByTestId(`batch-export-${format}`).click();
      await expect(page.getByTestId("batch-export-menu")).toHaveCount(0);
    }

    // THEN each export wrote the file chosen in the save dialog
    await expect.poll(() => callsOf(page, "export_batch")).toEqual([
      { batchId: "b-1", format: "xlsx", path: "/exports/batch-b-1.xlsx" },
      { batchId: "b-1", format: "csv", path: "/exports/batch-b-1.csv" },
      { batchId: "b-1", format: "json", path: "/exports/batch-b-1.json" },
    ]);

    // WHEN stop is clicked, the batch goes on until the confirmation
    await view.getByTestId("batch-stop").click();
    const confirm = page.getByTestId("batch-stop-dialog");
    await expect(confirm).toBeVisible();
    expect((await callsOf(page, "control_batch")).map((call) => call.action)).toEqual(["pause", "retry", "resume"]);
    await confirm.getByRole("button").last().click();

    // THEN it is stopped: the items left are not processed, and only the export remains
    await expect(view.getByTestId("batch-state")).toHaveAttribute("data-state", "stopped");
    await expect(view.getByTestId("batch-count-canceled")).toContainText("4");
    await expect(view.getByTestId("batch-export")).toBeVisible();
    await expect(view.getByTestId("batch-retry")).toHaveCount(0);
    await expect(view.getByTestId("batch-stop")).toHaveCount(0);
    expect((await callsOf(page, "control_batch")).at(-1)).toEqual({ batchId: "b-1", action: "stop" });
  });

  test("a waiting item opens its decision, and a row opens its detail in the sidebar", async ({ page }) => {
    // GIVEN a batch whose third item waits for an access nobody granted
    const view = await launchFromSheet(page);
    await endItem(page, 2, "awaiting", "Waiting for a decision: the agent asked to use 'send_email', which is not granted");
    const waiting = view.getByTestId("batch-item").nth(2);
    await expect(waiting).toHaveAttribute("data-status", "awaiting");
    await expect(waiting.getByTestId("batch-item-output")).toContainText("send_email");
    await expect(view.getByTestId("batch-count-awaiting")).toContainText("1");

    // WHEN its decision is opened and allowed
    await waiting.getByTestId("batch-item-decision").click();
    const decision = page.getByTestId("batch-decision-dialog");
    await expect(decision.getByTestId("batch-decision-summary")).toContainText("send an email");
    await expect(decision.getByTestId("batch-decision-refuse")).toBeVisible();
    await decision.getByTestId("batch-decision-always").click();

    // THEN the answer went through the list of waiting decisions, and the item runs again
    await expect(decision).toBeHidden();
    expect(await callsOf(page, "decide_batch_item")).toEqual([{ id: "batch:b-1:2", answer: "always" }]);
    await expect(waiting).toHaveAttribute("data-status", "pending");

    // WHEN a row that ran is opened
    await endItem(page, 0, "completed", "Quote, high priority");
    await view.getByTestId("batch-item").first().getByTestId("batch-item-open").click();

    // THEN the detail sidebar shows its whole input, its output, its steps and its sources
    const detail = page.getByTestId("detail-sidebar-item");
    await expect(page.getByTestId("detail-sidebar")).toHaveAttribute("data-tab", "item");
    await expect(detail.getByTestId("batch-item-detail-input")).toContainText("Request of Acme");
    await expect(detail.getByTestId("batch-item-detail-output")).toContainText("Quote, high priority");
    await detail.getByTestId("batch-item-detail-steps").locator("summary").click();
    await expect(detail.getByTestId("batch-item-detail-steps")).toContainText("read_page");
    await detail.getByTestId("batch-item-detail-sources").locator("summary").click();
    await expect(detail.getByTestId("batch-item-detail-sources")).toContainText("Sheet of Acme");
  });

  test("the end of a batch is announced by a system notification with its successes and failures", async ({ page }) => {
    // GIVEN the application open on a batch
    await launchFromSheet(page);

    // WHEN the batch engine reports the end of the batch
    await page.evaluate(() => {
      (globalThis as unknown as Globals).__engine.emit("task-changed", "BatchEnded", {
        batch_id: "b-1", agent_id: "triage", state: "completed", total: 5, completed: 4, failed: 1, canceled: 0, awaiting: 0,
      });
    });

    // THEN one system notification names the agent, 4 successes and 1 failure
    await expect.poll(() => page.evaluate(() => (globalThis as unknown as Globals).__notifications.length)).toBe(1);
    const sent = await page.evaluate(() => (globalThis as unknown as Globals).__notifications[0]);
    expect(sent.title).not.toBe("");
    expect(sent.body).toContain("triage");
    expect(sent.body).toMatch(/4\D+1\D/);
  });

  test("the history lists every batch past the first fifty and opens a past one", async ({ page }) => {
    // GIVEN 120 batches, opened from the Agents screen
    await page.goto("/");
    await page.getByTestId("nav-agents").first().click();
    await page.getByTestId("agents-batch-history").click();
    const history = page.getByTestId("batch-history-page");
    await expect(history).toBeVisible();

    // THEN the first page shows fifty batches and says the list goes on
    await expect(history.getByTestId("batch-history-row")).toHaveCount(50);
    await expect(history.getByTestId("batch-history-count")).toHaveAttribute("data-more", "true");

    // WHEN the end of the list is scrolled to, twice
    await history.getByTestId("batch-history-load-older").scrollIntoViewIfNeeded();
    await expect(history.getByTestId("batch-history-row")).toHaveCount(100);
    await history.getByTestId("batch-history-load-older").scrollIntoViewIfNeeded();

    // THEN the list read on by itself until the last batch: no ceiling of fifty
    await expect(history.getByTestId("batch-history-row")).toHaveCount(120);
    await expect(history.getByTestId("batch-history-count")).toHaveAttribute("data-more", "false");
    await expect(history.getByTestId("batch-history-count")).toContainText("120");
    expect((await callsOf(page, "list_batches")).map((call) => call.limit)).toEqual([51, 101, 151]);

    // WHEN the interrupted ones are asked for
    await history.getByTestId("batch-history-outcome").selectOption("interrupted");

    // THEN only those stay
    await expect(history.getByTestId("batch-history-row")).toHaveCount(40);
    await history.getByTestId("batch-history-outcome").selectOption("");

    // WHEN a past batch is opened
    await history.locator('[data-testid="batch-history-row"][data-batch="past-1"]').click();

    // THEN the view reads that batch, and offers the way back to the history
    const view = page.getByTestId("batch-page");
    await expect(view.getByTestId("batch-state")).toHaveAttribute("data-state", "running");
    expect((await callsOf(page, "get_batch")).at(-1)).toEqual({ batchId: "past-1" });
    await view.getByTestId("batch-to-history").click();
    await expect(page.getByTestId("batch-history-page")).toBeVisible();
  });
});
