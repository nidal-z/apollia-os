import { test, expect, type Page } from "@playwright/test";

/**
 * Creating an agent by describing it.
 *
 * Boots the UI bundle with the Tauri bridge stubbed, as
 * `tests/agents/workflow-launch.spec.ts` does, and plays the whole journey
 * from the Agents screen: "Create an agent" opens a conversation with
 * Apollia, a scripted model stands in for the real one and answers the
 * description with a call of `propose_agent`, the draft fills up beside the
 * conversation, opens in the form of an agent sheet, and is saved.
 *
 * The scripted model lives in the stub: the first message of a conversation
 * gets a question, the second one gets the draft, as the output of the tool
 * call the runtime would have run. The stub records every command it is sent,
 * so the spec can tell what was written, and when.
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
    interface ToolCall { tool_name: string; input: unknown; output: string; status: string }
    interface Message {
      id: string; role: string; content: string; tool_calls: ToolCall[] | null;
      tool_name: null; seq: number; created_at: string;
    }
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
    const sessions = new Map<string, Message[]>();
    let stored: Record<string, unknown> | null = null;
    const say = (messages: Message[], role: string, content: string, calls: ToolCall[] | null = null) => {
      messages.push({
        id: `m${messages.length + 1}`, role, content, tool_calls: calls, tool_name: null,
        seq: messages.length + 1, created_at: "2026-10-05T09:00:00Z",
      });
    };
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
        const id = `session-${sessions.size + 1}`;
        sessions.set(id, []);
        return {
          id, mode: request.mode, agent_name: request.agent_name ?? null, status: "active",
          last_message_preview: null, message_count: 0, created_at: "2026-10-05T09:00:00Z",
          closed_at: null, title: null,
        };
      },
      // The scripted model: a question first, then the draft.
      send_chat_message: (args) => {
        const { sessionId, content } = args as { sessionId: string; content: string };
        const messages = sessions.get(sessionId);
        if (!messages) throw "no such session";
        say(messages, "user", content);
        const turn = messages.filter((m) => m.role === "user").length;
        if (turn === 1) {
          say(messages, "assistant", "What should this agent do for you?");
        } else {
          say(messages, "assistant", "Here is a first draft of the sheet.", [{
            tool_name: "propose_agent",
            input: {},
            output: JSON.stringify({ ok: true, draft }),
            status: "executed",
          }]);
        }
        return `m${messages.length}`;
      },
      get_chat_session: (args) => {
        const id = (args as { sessionId: string }).sessionId;
        const messages = sessions.get(id);
        if (!messages) return null;
        return {
          id, mode: "libre", agent_name: null, system_prompt: "", status: "active",
          available_tools: ["propose_agent"], authorized_tools: [], messages,
          created_at: "2026-10-05T09:00:00Z", closed_at: null, llm_backend: null, title: null,
          plan_mode: false, plan_phase: "done",
        };
      },
      delete_chat_session: (args) => {
        sessions.delete((args as { sessionId: string }).sessionId);
        return null;
      },
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
  await expect(page.getByTestId("agent-create")).toHaveAttribute("data-phase", "conversation");
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

/** Answer Apollia's question, which makes the scripted model propose. */
async function describeTheAgent(page: Page): Promise<void> {
  const conversation = page.getByTestId("agent-create-conversation");
  await expect(conversation).toContainText("What should this agent do for you?");
  await conversation.locator("textarea").fill("Read the press every morning and mail me a summary");
  await conversation.getByTestId("chat-send-button").click();
}

test.describe("creating an agent by describing it", () => {
  test.beforeEach(async ({ page }) => {
    await installTauriStub(page);
  });

  test("the journey: a conversation with Apollia, a draft, the form, then the agent", async ({ page }) => {
    // GIVEN the Agents screen
    // WHEN "Create an agent" is chosen
    await openCreation(page);

    // THEN a conversation with Apollia opens, offering the draft tool alone,
    //      and the draft beside it is empty and cannot be reviewed yet
    expect(await sent(page, "create_chat_session")).toEqual([
      { request: { mode: "libre", tools: ["propose_agent"] } },
    ]);
    const draft = page.getByTestId("agent-create-draft");
    await expect(draft.getByTestId("agent-create-draft-field")).toHaveCount(6);
    await expect(draft.locator('[data-state="filled"]')).toHaveCount(0);
    await expect(page.getByTestId("agent-create-review-open")).toBeDisabled();

    // WHEN the person describes the agent and the model proposes a draft
    await describeTheAgent(page);

    // THEN the draft fills up: name, mission, examples, model, accesses, sources
    await expect(draft.locator('[data-field="name"]')).toHaveAttribute("data-state", "filled");
    await expect(draft.locator('[data-field="name"]')).toContainText("Press watch");
    await expect(draft.locator('[data-field="mission"]')).toContainText("Summarise the press");
    await expect(draft.locator('[data-state="filled"]')).toHaveCount(6);
    await expect(draft.getByTestId("agent-create-unavailable")).toContainText("calendar_read");

    // WHEN the draft is reviewed
    await page.getByTestId("agent-create-review-open").click();

    // THEN it opens in the form of a sheet: the name, the mission, each access
    //      in its family, the sensitive one unchecked, the default one checked
    const review = page.getByTestId("agent-create-review");
    await expect(review).toBeVisible();
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

  test("the edits of the form survive a look back at the conversation", async ({ page }) => {
    // GIVEN a draft opened in the form, its sensitive access checked by hand
    await openCreation(page);
    await describeTheAgent(page);
    await expect(page.getByTestId("agent-create-review-open")).toBeEnabled();
    await page.getByTestId("agent-create-review-open").click();
    const review = page.getByTestId("agent-create-review");
    const mail = review.locator('[data-testid="agent-adjust-access-row"][data-access="mail_send"]');
    await mail.getByRole("switch").click();
    await expect(mail.getByRole("switch")).toHaveAttribute("aria-checked", "true");

    // WHEN the person goes back to the conversation, then reviews again
    await page.getByTestId("agent-create-review-back").click();
    await expect(page.getByTestId("agent-create")).toHaveAttribute("data-phase", "conversation");
    await page.getByTestId("agent-create-review-open").click();

    // THEN the access is still checked, and saving sends it
    await expect(mail.getByRole("switch")).toHaveAttribute("aria-checked", "true");
    await page.getByTestId("agent-create-save").click();
    await expect(page.getByTestId("agent-create")).toHaveAttribute("data-phase", "saved");
    const [{ sheet }] = (await sent(page, "create_agent_definition")) as [{ sheet: { access: string[] } }];
    expect(sheet.access).toEqual(["notes_lookup", "web_search", "mail_send"]);
  });

  test("leaving before the save creates no agent and drops the conversation", async ({ page }) => {
    // GIVEN a draft read over in the form
    await openCreation(page);
    await describeTheAgent(page);
    await expect(page.getByTestId("agent-create-review-open")).toBeEnabled();
    await page.getByTestId("agent-create-review-open").click();
    await expect(page.getByTestId("agent-create-review")).toBeVisible();

    // WHEN the person leaves without saving
    await page.getByTestId("agent-create-back").click();

    // THEN the list is back, no agent was created, and the conversation is removed
    await expect(page.getByTestId("agents-list")).toBeVisible();
    expect(await sent(page, "create_agent_definition")).toEqual([]);
    await expect.poll(async () => await sent(page, "delete_chat_session")).toEqual([{ sessionId: "session-1" }]);
  });
});
