import { test, expect, type Page } from "@playwright/test";

/**
 * The Agents screen lists agents and workflows side by side, and the sheet of
 * a workflow launches it.
 *
 * Boots the UI bundle with the Tauri bridge stubbed (the three members the
 * boot path reaches for), reaches the Agents screen through its sidebar
 * entry, opens the sheet of one workflow and launches it through the form
 * generated from its inputs. The stub keeps the runs it is asked to launch:
 * a launched run has its first step done and its second in progress until it
 * is stopped, and a launch whose `client` is `nobody` is refused the way the
 * workflow engine refuses an input.
 */

type InvokeStub = (cmd: string, args: unknown) => unknown;

interface TauriInternalsStub {
  invoke: InvokeStub;
  transformCallback: (cb: (payload: unknown) => void, once: boolean) => number;
  convertFileSrc: (path: string) => string;
}

interface Call {
  cmd: string;
  args: Record<string, unknown>;
}

async function installTauriStub(page: Page): Promise<void> {
  await page.addInitScript(() => {
    const agent = {
      id: "rt-invoices", name: "invoices", version: "1.2.0", enabled: true, runtime_status: "active",
      installed_at: "2026-10-01T09:00:00Z", description: "Sorts the invoices of the week", tags: [],
      tools_required: [], tools_optional: [], execution_mode: "direct", install_path: "/agents/invoices",
      supports_a2a: false, skills: [], agent_type: "assistant", examples: [], limitations: [],
      setup_notes: null, class_name: "Invoices", memory_namespace: "invoices", shared_memory_namespaces: [],
    };
    const workflow = {
      workflowId: "sales/follow-up",
      version: "1.2.0",
      description: "Reminds the clients whose quote was left unanswered.",
      steps: ["collect", "draft", "send"],
      inputs: [
        { name: "client", kind: "text", required: true, nullable: false, description: "Who to remind" },
        { name: "delay", kind: "integer", required: true, nullable: false },
        { name: "notify", kind: "boolean", required: false, nullable: false },
        { name: "level", kind: "choice", required: false, nullable: false, choices: ["low", "high"] },
        { name: "notes", kind: "long_text", required: false, nullable: false },
        { name: "tags", kind: "list", required: false, nullable: false, itemKind: "text" },
      ],
    };
    interface Run {
      runId: string; workflowId: string; parentRunId: null; depth: number; inputs: unknown;
      status: string; output: unknown; error: string | null; startedAt: string; endedAt: string | null;
    }
    const runs: Run[] = [{
      runId: "run-1", workflowId: "sales/follow-up", parentRunId: null, depth: 1,
      inputs: { client: "Acme", delay: 7 }, status: "completed", output: { sent: 3 }, error: null,
      startedAt: "2026-10-03T09:00:00Z", endedAt: "2026-10-03T09:01:30Z",
    }];
    const stepsOf = (run: Run) => {
      const states = run.status === "running"
        ? ["completed", "running", "pending"]
        : run.status === "canceled"
          ? ["completed", "canceled", "not_started"]
          : ["completed", "completed", "completed"];
      return workflow.steps.map((name, rank) => ({
        rank, name, status: states[rank],
        startedAt: states[rank] === "pending" || states[rank] === "not_started" ? null : run.startedAt,
        endedAt: states[rank] === "completed" ? run.startedAt : null,
        output: states[rank] === "completed" && rank === 0 ? "Three quotes found" : null,
        error: null,
      }));
    };
    const calls: Call[] = [];
    (globalThis as unknown as { __workflowCalls: Call[] }).__workflowCalls = calls;
    const handlers: Record<string, (args: unknown) => unknown> = {
      get_onboarding_state: () => ({ completed: true, skipped: false, started_at: null }),
      read_config_file: () => ({ path: "~/.apollia/apollia.toml", exists: true, text: "", sha256: "", diagnostics: [], locked: [], pendingRestart: [] }),
      get_system_info: () => ({}),
      list_agents: () => [agent],
      list_workflows: () => [
        { workflowId: "sales/follow-up", name: "Follow up the quotes", description: workflow.description },
      ],
      get_workflow: () => workflow,
      list_workflow_runs: (args) => {
        const limit = (args as { limit?: number }).limit;
        return limit ? runs.slice(0, limit) : runs;
      },
      get_workflow_run: (args) => {
        const run = runs.find((r) => r.runId === (args as { runId: string }).runId);
        return run ? { run, steps: stepsOf(run) } : null;
      },
      launch_workflow: (args) => {
        const inputs = (args as { inputs: Record<string, unknown> }).inputs;
        if (inputs.client === "nobody") {
          throw "input 'client': no client by that name [not_a_choice]";
        }
        const run: Run = {
          runId: `run-${runs.length + 1}`, workflowId: "sales/follow-up", parentRunId: null, depth: 1,
          inputs, status: "running", output: null, error: null,
          startedAt: new Date().toISOString(), endedAt: null,
        };
        runs.unshift(run);
        return run;
      },
      cancel_workflow_run: (args) => {
        const run = runs.find((r) => r.runId === (args as { runId: string }).runId);
        if (!run) throw "no such run";
        run.status = "canceled";
        run.error = "stopped";
        run.endedAt = new Date().toISOString();
        return run;
      },
      get_agent_package_detail: () => ({
        name: "sales", version: "1.2.0", description: "", author: "", agents: [], installed_at: "", updated_at: "",
        root_path: "", root_missing: false,
        manifest: { workflows: [{ name: "follow-up", action: "send_external" }] },
        signature: { publisher: "Acme", signed: true, trusted: true, key_fingerprint: null },
        embedded: false,
        content: {
          agents: [], workflows: [], tools: [], connections_brought: [],
          connections_required: [{ type: "gmail", reason: "Sends the reminders" }],
        },
        automations: [],
      }),
    };
    let nextCallbackId = 1;
    (globalThis as unknown as { __TAURI_INTERNALS__?: TauriInternalsStub }).__TAURI_INTERNALS__ = {
      invoke: (cmd: string, args: unknown) => {
        calls.push({ cmd, args: (args ?? {}) as Record<string, unknown> });
        const fn = handlers[cmd];
        if (fn) {
          try {
            return Promise.resolve(fn(args));
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

async function openAgents(page: Page): Promise<void> {
  await page.goto("/");
  await page.getByTestId("nav-agents").first().click();
  await expect(page.getByTestId("agents-page")).toBeVisible();
}

async function launches(page: Page): Promise<Record<string, unknown>[]> {
  return page.evaluate(() =>
    (globalThis as unknown as { __workflowCalls: Call[] }).__workflowCalls
      .filter((call) => call.cmd === "launch_workflow")
      .map((call) => call.args),
  );
}

test.describe("agents and workflows", () => {
  test.beforeEach(async ({ page }) => {
    await installTauriStub(page);
  });

  test("the screen lists agents and workflows side by side, without package tabs nor a Python file picker", async ({ page }) => {
    // GIVEN one installed agent and one installed workflow
    // WHEN the Agents screen opens
    await openAgents(page);

    // THEN each has its column, on the same row of the screen
    const agents = page.getByTestId("agents-column");
    const workflows = page.getByTestId("workflows-column");
    await expect(agents.getByTestId("agent-list-row")).toHaveCount(1);
    await expect(workflows.getByTestId("workflow-row")).toHaveCount(1);
    await expect(workflows.getByTestId("workflow-row")).toContainText("Follow up the quotes");
    const [left, right] = await Promise.all([agents.boundingBox(), workflows.boundingBox()]);
    expect(left && right && Math.abs(left.y - right.y) < 2 && right.x > left.x + left.width / 2).toBe(true);

    // AND the screen holds no tab, and one install gesture only, the package
    // file, beside the creation of an agent
    const header = page.getByTestId("agents-list").locator("h1").locator("xpath=../..");
    await expect(page.getByTestId("agents-page").getByRole("tab")).toHaveCount(0);
    await expect(header.getByRole("button")).toHaveCount(2);
    await expect(header.getByTestId("agents-install-apo-button")).toBeVisible();
    await expect(header.getByTestId("agents-create-button")).toBeVisible();
  });

  test("the sheet of a workflow shows what it does, its steps, its accesses and its history", async ({ page }) => {
    // GIVEN the Agents screen
    await openAgents(page);

    // WHEN the workflow is opened
    await page.getByTestId("workflow-list-row").click();
    const sheet = page.getByTestId("workflow-sheet");
    await expect(sheet).toBeVisible();

    // THEN the sheet reads what it does, its three steps, what it may do and its past run
    await expect(sheet.getByTestId("workflow-sheet-what")).toContainText("Reminds the clients");
    await expect(sheet.getByTestId("workflow-step")).toHaveCount(3);
    await expect(sheet.getByTestId("workflow-step").first()).toHaveAttribute("data-status", "declared");
    await expect(sheet.getByTestId("workflow-access-family")).toHaveAttribute("data-family", "send_external");
    await expect(sheet.getByTestId("workflow-access-connection")).toContainText("gmail");
    await expect(sheet.getByTestId("workflow-run")).toHaveCount(1);
    await expect(sheet.getByTestId("workflow-run")).toHaveAttribute("data-status", "completed");

    // AND the way back leads to the list
    await sheet.getByTestId("workflow-sheet-back").click();
    await expect(page.getByTestId("agents-list")).toBeVisible();
  });

  test("launching fills the form generated from the inputs, shows the run step by step, and stops it", async ({ page }) => {
    // GIVEN the launch form of the workflow, opened from its row
    await openAgents(page);
    await page.getByTestId("workflow-row-launch").click();
    const dialog = page.getByTestId("workflow-launch-dialog");
    await expect(dialog).toBeVisible();

    // THEN one field per declared input, each with the control of its type
    const fields = dialog.locator("[data-field]");
    await expect(fields).toHaveCount(6);
    expect(await fields.evaluateAll((nodes) => nodes.map((n) => n.getAttribute("data-control")))).toEqual([
      "text", "number", "toggle", "select", "textarea", "items",
    ]);

    // WHEN it is launched with its required inputs empty
    await dialog.getByTestId("workflow-launch-confirm").click();

    // THEN nothing is launched and both fields at fault are marked
    await expect(dialog.getByTestId("workflow-launch-errors")).toBeVisible();
    await expect(dialog.locator('[data-field][data-invalid="true"]')).toHaveCount(2);
    expect(await launches(page)).toHaveLength(0);

    // WHEN the form is filled and launched
    await dialog.getByTestId("workflow-input-client").fill("Acme");
    await dialog.getByTestId("workflow-input-delay").fill("7");
    await dialog.getByTestId("workflow-input-level").selectOption("high");
    await dialog.getByTestId("workflow-input-tags-draft").fill("urgent");
    await dialog.getByTestId("workflow-input-tags-add").click();
    await dialog.getByTestId("workflow-launch-confirm").click();

    // THEN the inputs go typed, and the run shows above the steps, each with its state
    await expect(dialog).toBeHidden();
    expect(await launches(page)).toEqual([{
      workflowId: "sales/follow-up",
      inputs: { client: "Acme", delay: 7, notify: false, level: "high", tags: ["urgent"] },
    }]);
    const card = page.getByTestId("workflow-run-card");
    await expect(card).toHaveAttribute("data-status", "running");
    const steps = page.getByTestId("workflow-step");
    await expect(steps.nth(0)).toHaveAttribute("data-status", "completed");
    await expect(steps.nth(1)).toHaveAttribute("data-status", "running");
    await expect(steps.nth(2)).toHaveAttribute("data-status", "pending");
    await expect(page.getByTestId("workflow-run").first()).toHaveAttribute("data-status", "running");

    // WHEN the run is stopped, after its confirmation
    await card.getByTestId("workflow-run-stop").click();
    await page.getByTestId("workflow-run-stop-dialog-confirm").click();

    // THEN the run is stopped: its step in progress stopped, the next one not launched
    await expect(card).toHaveAttribute("data-status", "canceled");
    await expect(steps.nth(1)).toHaveAttribute("data-status", "canceled");
    await expect(steps.nth(2)).toHaveAttribute("data-status", "not_started");
    await expect(card.getByTestId("workflow-run-stop")).toHaveCount(0);

    // AND launching again opens the form filled with the inputs of that run
    await card.getByTestId("workflow-run-relaunch").click();
    await expect(dialog.getByTestId("workflow-input-client")).toHaveValue("Acme");
    await expect(dialog.getByTestId("workflow-input-delay")).toHaveValue("7");
  });

  test("an input the workflow refuses is marked on its field, and the form stays open", async ({ page }) => {
    // GIVEN the launch form, filled with a client the workflow does not know
    await openAgents(page);
    await page.getByTestId("workflow-list-row").click();
    await page.getByTestId("workflow-launch-button").click();
    const dialog = page.getByTestId("workflow-launch-dialog");
    await dialog.getByTestId("workflow-input-client").fill("nobody");
    await dialog.getByTestId("workflow-input-delay").fill("3");

    // WHEN it is launched
    await dialog.getByTestId("workflow-launch-confirm").click();

    // THEN the form stays open, the field is marked, and the refusal is quoted
    await expect(dialog).toBeVisible();
    await expect(dialog.locator('[data-field="client"]')).toHaveAttribute("data-invalid", "true");
    await expect(dialog.getByTestId("workflow-launch-refused")).toHaveText("no client by that name");
    await expect(page.getByTestId("workflow-run-card")).toHaveCount(0);
  });
});
