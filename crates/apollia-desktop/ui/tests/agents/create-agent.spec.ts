import { test, expect, type Page } from "@playwright/test";

/**
 * Creating an agent from a short form.
 *
 * Boots the UI bundle with the Tauri bridge stubbed, as
 * `tests/agents/workflow-launch.spec.ts` does, and plays the whole journey
 * from the Agents screen: "Create an agent" opens a form, the agent workshop
 * is launched on it, asks one question, hands a sheet back, the runtime
 * checks it, the draft opens in the form of an agent sheet, and is saved.
 *
 * The workshop lives in the stub: its run waits on a question until it is
 * answered, then completes with a sheet. The stub records every command it
 * is sent, so the spec can tell what was written, and when.
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
    const families: Record<string, string> = {
      web_search: "read",
      notes_lookup: "read",
      mail_send: "send_external",
    };
    const draft = {
      name: "Press watch",
      mission: "Summarise the press every morning.",
      examples: ["What is new today?"],
      model: null,
      access: [
        { id: "notes_lookup", checked: true, sensitive: false, by_default: false },
        { id: "mail_send", checked: false, sensitive: true, by_default: false },
        { id: "web_search", checked: true, sensitive: false, by_default: true },
      ],
      sources: ["profile"],
      unavailable: ["calendar_read"],
    };
    const offer = {
      accesses: [
        { id: "web_search", description: "Does web_search", family: "read", sensitive: false, by_default: true },
        { id: "notes_lookup", description: "Does notes_lookup", family: "read", sensitive: false, by_default: false },
        { id: "mail_send", description: "Does mail_send", family: "send_external", sensitive: true, by_default: false },
      ],
      sources: [
        { id: "profile", label: "the profile of the person" },
        { id: "memory", label: "the memory the agent keeps across its runs" },
      ],
      models: [],
    };
    // The run of the workshop: waiting on its question until it is answered.
    let answered = false;
    let stored: Record<string, unknown> | null = null;
    const step = (rank: number, name: string, status: string) => ({
      rank, name, status, startedAt: null, endedAt: null, output: null, error: null,
    });
    const run = (status: string, output: unknown) => ({
      runId: "run-1", workflowId: "agent-builder/draft", parentRunId: null, depth: 0, inputs: {},
      status, output, error: null, startedAt: "2026-10-05T09:00:00Z", endedAt: null,
    });
    const calls: Call[] = [];
    (globalThis as unknown as { __createCalls: Call[] }).__createCalls = calls;
    const handlers: Record<string, (args: unknown) => unknown> = {
      get_onboarding_state: () => ({ completed: true, skipped: false, started_at: null }),
      read_config_file: () => ({ path: "~/.apollia/apollia.toml", exists: true, text: "", sha256: "", diagnostics: [], locked: [], pendingRestart: [] }),
      get_system_info: () => ({}),
      list_agents: () => [],
      list_llm_backends: () => [
        { name: "local", provider: "llama-cpp", model: "qwen", enabled: true, is_default: true, config_json: {} },
      ],
      create_chat_session: (args) => {
        const request = (args as { request: { mode: string; agent_name?: string } }).request;
        return {
          id: "session-1", mode: request.mode, agent_name: request.agent_name ?? null, status: "active",
          last_message_preview: null, message_count: 0, created_at: "2026-10-05T09:00:00Z",
          closed_at: null, title: null,
        };
      },
      agent_draft_offer: () => offer,
      launch_workflow: () => {
        answered = false;
        return run("running", null);
      },
      get_workflow_run: () => ({
        run: answered
          ? run("completed", {
              name: draft.name, mission: draft.mission, examples: draft.examples,
              access: ["notes_lookup", "mail_send", "calendar_read"], sources: ["profile"], answers: [],
            })
          : run("running", null),
        steps: [
          step(0, "understand", "completed"),
          step(1, "ask", answered ? "completed" : "running"),
          step(2, "write", answered ? "completed" : "pending"),
        ],
      }),
      list_pending_decisions: () => ({
        items: answered
          ? []
          : [{
              id: "workflow-question:r1", kind: "question", agentId: null,
              summary: "How long should the summary be?", access: null, family: null,
              answers: ["answer", "refuse"],
              questions: [{
                id: "answer", question: "How long should the summary be?", type: "single_choice",
                options: ["Five lines", "One page"],
              }],
              allowOther: true, origin: { requestId: "r1", runId: "run-1" }, askedAt: null,
            }],
      }),
      decide_pending_decision: () => {
        answered = true;
        return { id: "workflow-question:r1", answer: "answer", resumed: 1, batchItemsResumed: 0, message: "" };
      },
      cancel_workflow_run: () => run("canceled", null),
      // The runtime checks the sheet: what the machine lacks is named.
      check_agent_draft: () => draft,
      describe_tool: (args) => {
        const name = (args as { name: string }).name;
        return families[name]
          ? {
              name, version: "1", description: `Does ${name}`, kind: { type: "native" },
              input_schema: null, output_schema: null, permissions: [],
              family: families[name], family_source: "builtin",
            }
          : null;
      },
      create_agent_definition: (args) => {
        const sheet = (args as { sheet: Record<string, unknown> }).sheet;
        stored = {
          id: "press-watch", origin: "interface", package: null, version: "1", enabled: true,
          ...sheet, model: null, autonomy: "supervised", profile: null, overridden: [],
          definition: { id: "press-watch", ...sheet },
        };
        return stored;
      },
      get_agent_definition: (args) => ((args as { name: string }).name === "press-watch" ? stored : null),
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

async function openCreation(page: Page): Promise<void> {
  await page.goto("/");
  await page.getByTestId("nav-agents").first().click();
  await expect(page.getByTestId("agents-page")).toBeVisible();
  await page.getByTestId("agents-create-button").click();
  await expect(page.getByTestId("agent-create")).toHaveAttribute("data-phase", "form");
}

async function sent(page: Page, cmd: string): Promise<Record<string, unknown>[]> {
  return page.evaluate(
    (name) =>
      (globalThis as unknown as { __createCalls: Call[] }).__createCalls
        .filter((call) => call.cmd === name)
        .map((call) => call.args),
    cmd,
  );
}

/** Fill the form, launch the workshop and answer its question. */
async function prepareTheSheet(page: Page): Promise<void> {
  await page.getByTestId("agent-create-purpose").fill("Read the press every morning and mail me a summary");
  await page.getByTestId("agent-create-prepare").click();
  await expect(page.getByTestId("agent-create-question-text")).toContainText("How long should the summary be?");
  await page.getByTestId("agent-create-question-option").first().click();
  await expect(page.getByTestId("agent-create-review")).toBeVisible();
}

test.describe("creating an agent from a form", () => {
  test.beforeEach(async ({ page }) => {
    await installTauriStub(page);
  });

  test("the journey: a form, a question of the workshop, the sheet, then the agent", async ({ page }) => {
    // GIVEN the Agents screen
    // WHEN "Create an agent" is chosen
    await openCreation(page);

    // THEN a form opens, which cannot be launched while it says nothing,
    //      and lists the sources of the machine and the rights it offers
    await expect(page.getByTestId("agent-create-prepare")).toBeDisabled();
    await expect(page.getByTestId("agent-create-source")).toHaveCount(2);
    await expect(page.getByTestId("agent-create-right")).toHaveCount(1);
    expect(await sent(page, "launch_workflow")).toEqual([]);

    // WHEN the person says what the agent is for, checks a source and a right,
    //      and launches the workshop
    await page.getByTestId("agent-create-purpose").fill("Read the press every morning and mail me a summary");
    await page.getByTestId("agent-create-source").first().click();
    await page.getByTestId("agent-create-right").first().click();
    await page.getByTestId("agent-create-prepare").click();

    // THEN the workshop is launched on the form, with what the machine holds
    await expect(page.getByTestId("agent-create")).toHaveAttribute("data-phase", "drafting");
    const [launch] = (await sent(page, "launch_workflow")) as [{ workflowId: string; inputs: Record<string, unknown> }];
    expect(launch.workflowId).toBe("agent-builder/draft");
    expect(launch.inputs.purpose).toBe("Read the press every morning and mail me a summary");
    expect(launch.inputs.sources).toEqual(["profile"]);
    expect(launch.inputs.rights).toEqual(["send_external"]);
    expect(JSON.parse(launch.inputs.offer as string).accesses).toHaveLength(3);

    // AND it asks its question here, with its proposed answers
    await expect(page.getByTestId("agent-create-question-text")).toContainText("How long should the summary be?");
    await expect(page.getByTestId("agent-create-question-option")).toHaveCount(2);
    await expect(page.locator('[data-testid="agent-create-step"][data-step="ask"]')).toHaveAttribute(
      "data-state",
      "current",
    );

    // WHEN the person picks an answer
    await page.getByTestId("agent-create-question-option").first().click();

    // THEN the answer goes to the run, the sheet it hands back is checked by
    //      the runtime, and the draft opens in the form of a sheet: each
    //      access in its family, the sensitive one unchecked, the default one
    //      checked, what the machine lacks named
    expect(await sent(page, "decide_pending_decision")).toEqual([
      { request: { id: "workflow-question:r1", answer: "answer", value: "Five lines" } },
    ]);
    const review = page.getByTestId("agent-create-review");
    await expect(review).toBeVisible();
    expect(await sent(page, "check_agent_draft")).toEqual([
      {
        sheet: {
          name: "Press watch",
          mission: "Summarise the press every morning.",
          examples: ["What is new today?"],
          access: ["notes_lookup", "mail_send", "calendar_read"],
          sources: ["profile"],
        },
      },
    ]);
    await expect(review.getByTestId("agent-create-name")).toHaveValue("Press watch");
    await expect(review.getByTestId("agent-adjust-mission")).toHaveValue("Summarise the press every morning.");
    const row = (id: string) => review.locator(`[data-testid="agent-adjust-access-row"][data-access="${id}"]`);
    await expect(row("notes_lookup").getByRole("switch")).toHaveAttribute("aria-checked", "true");
    await expect(row("web_search").getByRole("switch")).toHaveAttribute("aria-checked", "true");
    await expect(row("web_search").getByTestId("agent-adjust-access-note")).toBeVisible();
    await expect(row("mail_send").getByRole("switch")).toHaveAttribute("aria-checked", "false");
    await expect(row("mail_send").getByTestId("agent-adjust-access-note")).toBeVisible();
    await expect(review.locator('[data-family="send_external"]')).toContainText("Does mail_send");
    await expect(review.locator('[data-testid="agent-adjust-access-row"][data-access="calendar_read"]')).toHaveCount(0);
    await expect(page.getByTestId("agent-create-unavailable")).toContainText("calendar_read");

    // AND nothing was written so far
    expect(await sent(page, "create_agent_definition")).toEqual([]);

    // WHEN the name is changed and the form saved
    await review.getByTestId("agent-create-name").fill("Morning press");
    await page.getByTestId("agent-create-save").click();

    // THEN one agent is created from the sheet as validated, the sensitive
    //      access left out, and its sheet is shown
    await expect(page.getByTestId("agent-create")).toHaveAttribute("data-phase", "saved");
    expect(await sent(page, "create_agent_definition")).toEqual([
      {
        sheet: {
          name: "Morning press",
          mission: "Summarise the press every morning.",
          examples: ["What is new today?"],
          access: ["notes_lookup", "web_search"],
          sources: ["profile"],
        },
      },
    ]);
    await expect(page.getByTestId("agent-create-saved")).toContainText("Morning press");
    await expect(page.getByTestId("agent-sheet")).toHaveAttribute("data-agent", "press-watch");

    // WHEN the agent is tried
    await page.getByTestId("agent-create-try").click();

    // THEN a conversation with that agent is asked for
    await expect
      .poll(async () => (await sent(page, "create_chat_session")).at(-1))
      .toEqual({ request: { mode: "agent", agent_name: "press-watch" } });
  });

  test("the edits of the sheet survive a look back at the form", async ({ page }) => {
    // GIVEN a sheet opened in the form, its sensitive access checked by hand
    await openCreation(page);
    await prepareTheSheet(page);
    const review = page.getByTestId("agent-create-review");
    const mail = review.locator('[data-testid="agent-adjust-access-row"][data-access="mail_send"]');
    await mail.getByRole("switch").click();
    await expect(mail.getByRole("switch")).toHaveAttribute("aria-checked", "true");

    // WHEN the person goes back to the form, which kept what was typed, then
    //      returns to the sheet
    await page.getByTestId("agent-create-review-back").click();
    await expect(page.getByTestId("agent-create")).toHaveAttribute("data-phase", "form");
    await expect(page.getByTestId("agent-create-purpose")).toHaveValue(
      "Read the press every morning and mail me a summary",
    );
    await page.getByTestId("agent-create-review-resume").click();

    // THEN the access is still checked, and saving sends it
    await expect(mail.getByRole("switch")).toHaveAttribute("aria-checked", "true");
    await page.getByTestId("agent-create-save").click();
    await expect(page.getByTestId("agent-create")).toHaveAttribute("data-phase", "saved");
    const [{ sheet }] = (await sent(page, "create_agent_definition")) as [{ sheet: { access: string[] } }];
    expect(sheet.access).toEqual(["notes_lookup", "web_search", "mail_send"]);
  });

  test("leaving while the workshop works stops its run and creates no agent", async ({ page }) => {
    // GIVEN the workshop launched, waiting on its question
    await openCreation(page);
    await page.getByTestId("agent-create-purpose").fill("Read the press every morning");
    await page.getByTestId("agent-create-prepare").click();
    await expect(page.getByTestId("agent-create-question")).toBeVisible();

    // WHEN the person leaves without answering
    await page.getByTestId("agent-create-back").click();

    // THEN the list is back, the run was stopped and no agent was created
    await expect(page.getByTestId("agents-list")).toBeVisible();
    await expect.poll(async () => await sent(page, "cancel_workflow_run")).toEqual([{ runId: "run-1" }]);
    expect(await sent(page, "create_agent_definition")).toEqual([]);
  });
});
