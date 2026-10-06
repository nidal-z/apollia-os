import { test, expect, type Page } from "@playwright/test";

/**
 * A batch is launched from the sheet of an agent, or on a file dropped in the
 * chat, through the same dialog.
 *
 * Boots the UI bundle with the Tauri bridge stubbed (the three members the
 * boot path reaches for). The registry of sources holds the test source, a
 * CSV file, beside a folder and a mailbox; the agent lists one access a
 * launch may grant and one asked at every use. A workbook dropped in the chat
 * is previewed: two sheets, five columns, twenty rows shown out of 128 items.
 * The stub keeps every call, so a test reads the body the launch sent.
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

const TEST_SOURCE = "/data/test-source.csv";

async function installTauriStub(page: Page): Promise<void> {
  await page.addInitScript(() => {
    const agent = {
      id: "rt-triage", name: "triage", version: "1.0.0", enabled: true, runtime_status: "active",
      installed_at: "2026-10-01T09:00:00Z", description: "Qualifies the requests", tags: [],
      execution_mode: "direct",
      supports_a2a: false, skills: [], agent_type: "assistant", examples: [], limitations: [],
      setup_notes: null, class_name: "Triage", memory_namespace: "triage", shared_memory_namespaces: [],
    };
    const session = {
      id: "session-1", mode: "agent", agent_name: "triage", status: "active",
      last_message_preview: null, message_count: 0, created_at: "2026-10-05T09:00:00Z",
      closed_at: null, title: null,
    };
    const calls: Call[] = [];
    (globalThis as unknown as { __batchCalls: Call[] }).__batchCalls = calls;
    const handlers: Record<string, (args: unknown) => unknown> = {
      get_onboarding_state: () => ({ completed: true, skipped: false, started_at: null }),
      read_config_file: () => ({ path: "~/.apollia/apollia.toml", exists: true, text: "", sha256: "", diagnostics: [], locked: [], pendingRestart: [] }),
      get_system_info: () => ({}),
      list_agents: () => [agent],
      list_chat_sessions: () => [session],
      create_chat_session: () => session,
      get_chat_session: () => ({
        ...session, system_prompt: "", available_tools: [], authorized_tools: [], messages: [],
        llm_backend: null, plan_mode: false, plan_phase: "done",
      }),
      list_batch_sources: () => [
        { id: "s-1", kind: "file", label: "Test source", reference: "/data/test-source.csv" },
        { id: "s-2", kind: "folder", label: "Clients", reference: "conn-folder-1" },
        { id: "s-3", kind: "mail", label: "Support", reference: "imap:support@example.org" },
        { id: "s-4", kind: "profile", label: "Profile", reference: null },
      ],
      preview_batch_source: (args) => {
        const asked = args as { path: string; sheet: string | null; headerRow: number | null };
        if (!asked.path.startsWith("/data/requests.")) return null;
        const old = asked.path.endsWith(".xls");
        const notes = asked.sheet === "Notes";
        const names = notes ? ["Note"] : ["Customer", "Request", "Priority", "Date", "Amount"];
        const header = asked.headerRow ?? 1;
        const rows = Array.from({ length: notes ? 2 : 20 }, (_, index) => ({
          row: index + 1,
          cells: names.map((name) => (index === 0 ? name : `${name} ${index + 1}`)),
        }));
        return {
          format: old ? "xls" : "xlsx", sourceType: "spreadsheet",
          sheets: [{ name: "March", nonEmptyRows: 129 }, { name: "Notes", nonEmptyRows: 2 }],
          proposedSheet: "March", sheet: notes ? "Notes" : "March",
          proposedHeaderRow: 1, headerRow: header,
          columns: names.map((name, index) => ({
            letter: "ABCDE"[index],
            name: header === 1 ? name : `${name} ${header}`,
          })),
          rows, estimatedItems: notes ? 1 : 129 - header,
          canModifyOriginal: !old, formattingNotKept: old,
        };
      },
      list_batch_accesses: () => [
        { accessId: "send_email", permits: "Send an email", family: "send_external", grantable: true },
        { accessId: "run_python", permits: "Run a script", family: "execute_code", grantable: false },
      ],
      launch_batch: (args) => {
        const launch = (args as { launch: { agent: string; maxFailures: number; grants: string[] } }).launch;
        if (launch.grants.includes("source:write")) throw "writing into the original file was refused";
        return { batchId: "b-1", agent: launch.agent, state: "running", total: 3, maxFailures: launch.maxFailures };
      },
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

async function launches(page: Page): Promise<unknown[]> {
  return page.evaluate(() =>
    (globalThis as unknown as { __batchCalls: Call[] }).__batchCalls
      .filter((call) => call.cmd === "launch_batch")
      .map((call) => call.args.launch),
  );
}

async function openOnDroppedFile(page: Page, name: string) {
  await page.goto("/");
  await page.getByTestId("nav-chat").first().click();
  await page.getByRole("button", { name: /^triage/ }).first().click({ position: { x: 24, y: 12 } });
  const composer = page.getByTestId("chat-input");
  await expect(composer).toBeVisible();
  await composer.evaluate((node, dropped) => {
    const file = new File(["client,request\nAcme,quote\n"], dropped);
    Object.defineProperty(file, "path", { value: `/data/${dropped}` });
    const drop = new Event("drop", { bubbles: true, cancelable: true });
    Object.defineProperty(drop, "dataTransfer", { value: { files: [file] } });
    (node.querySelector("textarea")?.parentElement ?? node).dispatchEvent(drop);
  }, name);
  await page.getByTestId("chat-batch-offer").click();
  const dialog = page.getByTestId("batch-launch-dialog");
  await expect(dialog).toBeVisible();
  return dialog;
}

async function openFromSheet(page: Page) {
  await page.goto("/");
  await page.getByTestId("nav-agents").first().click();
  await expect(page.getByTestId("agents-page")).toBeVisible();
  await page.getByTestId("agent-list-row").first().click();
  await page.getByTestId("agent-batch-btn").click();
  const dialog = page.getByTestId("batch-launch-dialog");
  await expect(dialog).toBeVisible();
  return dialog;
}

test.describe("launching a batch", () => {
  test.beforeEach(async ({ page }) => {
    await installTauriStub(page);
  });

  test("the sheet of an agent opens the dialog, which refuses a launch without a source", async ({ page }) => {
    // GIVEN the sheet of an agent
    // WHEN "Process a batch" is clicked
    const dialog = await openFromSheet(page);

    // THEN the dialog names the agent and offers the three families of sources
    await expect(dialog.getByTestId("batch-launch-title")).toContainText("triage");
    await expect(dialog.getByTestId("batch-launch-family-table")).toBeVisible();
    await expect(dialog.getByTestId("batch-launch-family-folder")).toBeVisible();
    await expect(dialog.getByTestId("batch-launch-family-mail")).toBeVisible();

    // AND without a source the launch is refused: the reason is written, the button inert
    await expect(dialog.getByTestId("batch-launch-refusal")).toHaveAttribute("data-refusal", "source_missing");
    await expect(dialog.getByTestId("batch-launch-source")).toHaveAttribute("data-missing", "true");
    await expect(dialog.getByTestId("batch-launch-confirm")).toBeDisabled();
    expect(await launches(page)).toHaveLength(0);
  });

  test("the sources are those of the registry, and a folder says why no batch reads it", async ({ page }) => {
    // GIVEN the dialog
    const dialog = await openFromSheet(page);

    // WHEN the folder family is chosen
    await dialog.getByTestId("batch-launch-family-folder").getByRole("radio").click();

    // THEN the folder of the registry is listed, with the reason it cannot be chosen
    const folder = dialog.getByTestId("batch-launch-source-option");
    await expect(folder).toHaveCount(1);
    await expect(folder).toContainText("Clients");
    await expect(folder.getByTestId("batch-launch-source-blocked")).toBeVisible();
    await expect(folder.getByRole("radio")).toBeDisabled();

    // WHEN the mailbox family is chosen, then its mailbox
    await dialog.getByTestId("batch-launch-family-mail").getByRole("radio").click();
    await expect(dialog.getByTestId("batch-launch-source-option")).toContainText("Support");
    await dialog.getByTestId("batch-launch-source-option").getByRole("radio").click();

    // THEN the launch is accepted, and a mailbox offers no result file
    await expect(dialog.getByTestId("batch-launch-source-chosen")).toContainText("imap:support@example.org");
    await expect(dialog.getByTestId("batch-launch-results")).toHaveCount(0);
    await expect(dialog.getByTestId("batch-launch-confirm")).toBeEnabled();
  });

  test("a launch on the test source sends the accesses granted, the threshold and a copy with the agent's columns", async ({ page }) => {
    // GIVEN the dialog, on the test source of the registry
    const dialog = await openFromSheet(page);
    await dialog.getByTestId("batch-launch-family-table").getByRole("radio").click();
    await dialog.getByTestId("batch-launch-source-option").getByRole("radio").click();

    // THEN the results go into a copy by default, and a CSV file cannot be written in place
    await expect(dialog.getByTestId("batch-launch-destination-copy").getByRole("radio")).toHaveAttribute("aria-checked", "true");
    await expect(dialog.getByTestId("batch-launch-destination-original").getByRole("radio")).toBeDisabled();
    await expect(dialog.getByTestId("batch-launch-original-unavailable")).toBeVisible();

    // AND the accesses are those of the agent: one to grant, one asked at every use
    await expect(dialog.getByTestId("batch-launch-access")).toHaveCount(1);
    await expect(dialog.getByTestId("batch-launch-access")).toContainText("Send an email");
    await expect(dialog.getByTestId("batch-launch-access-asked")).toContainText("Run a script");
    await expect(dialog.getByTestId("batch-launch-threshold")).toHaveValue("10");

    // WHEN the access is granted, the threshold lifted, the columns named, and the batch launched
    await dialog.getByTestId("batch-launch-access-grant").click();
    await dialog.getByTestId("batch-launch-threshold").selectOption("never");
    await dialog.getByTestId("batch-launch-columns").fill("Category, Urgency");
    await dialog.getByTestId("batch-launch-confirm").click();

    // THEN the launch goes whole, and the dialog closes
    await expect(dialog).toBeHidden();
    expect(await launches(page)).toEqual([{
      agent: "triage",
      source: {
        kind: "csv",
        location: TEST_SOURCE,
        options: {},
        write: { destination: "copy", columns: ["Category", "Urgency"] },
      },
      grants: ["send_email"],
      maxFailures: 4294967295,
    }]);
  });

  test("a file dropped in the chat offers to process each row, in the same dialog opened on it", async ({ page }) => {
    // GIVEN a conversation held with the agent
    await page.goto("/");
    await page.getByTestId("nav-chat").first().click();
    await page.getByRole("button", { name: /^triage/ }).first().click({ position: { x: 24, y: 12 } });
    const composer = page.getByTestId("chat-input");
    await expect(composer).toBeVisible();
    await expect(page.getByTestId("chat-batch-offer")).toHaveCount(0);

    // WHEN a workbook is dropped on the composer
    await composer.evaluate((node) => {
      const file = new File(["client,request\nAcme,quote\n"], "requests.xlsx");
      Object.defineProperty(file, "path", { value: "/data/requests.xlsx" });
      const drop = new Event("drop", { bubbles: true, cancelable: true });
      Object.defineProperty(drop, "dataTransfer", { value: { files: [file] } });
      (node.querySelector("textarea")?.parentElement ?? node).dispatchEvent(drop);
    });

    // THEN "Process each row" is offered beside the file
    await expect(page.getByTestId("chat-attachment-chip")).toContainText("requests.xlsx");
    await page.getByTestId("chat-batch-offer").click();

    // AND it opens the same dialog, on that file, whose original may be written on request
    const dialog = page.getByTestId("batch-launch-dialog");
    await expect(dialog.getByTestId("batch-launch-title")).toContainText("triage");
    await expect(dialog.getByTestId("batch-launch-source-chosen")).toContainText("/data/requests.xlsx");
    await expect(dialog.getByTestId("batch-launch-destination-original").getByRole("radio")).toBeEnabled();

    // WHEN the original is asked for and the runtime refuses the launch
    await dialog.getByTestId("batch-launch-destination-original").getByRole("radio").click();
    await dialog.getByTestId("batch-launch-confirm").click();

    // THEN the dialog stays open on the reason, and the launch carried the write of the original
    await expect(dialog.getByTestId("batch-launch-error")).toContainText("refused");
    const sent = (await launches(page)) as { grants: string[]; source: { write: { destination: string } } }[];
    expect(sent.at(-1)?.grants).toContain("source:write");
    expect(sent.at(-1)?.source.write.destination).toBe("original");
  });

  test("process each row opens the preview of the dropped workbook, and an unchecked column is not sent", async ({ page }) => {
    // GIVEN a workbook dropped in the chat
    // WHEN "Process each row" is clicked
    const dialog = await openOnDroppedFile(page, "requests.xlsx");

    // THEN the preview is that of the file: sheet and header row proposed, twenty rows, every column checked
    const preview = dialog.getByTestId("batch-preview");
    await expect(preview.getByTestId("batch-preview-sheet")).toHaveValue("March");
    await expect(preview.getByTestId("batch-preview-header-row")).toHaveValue("1");
    await expect(preview.getByTestId("batch-preview-row")).toHaveCount(20);
    await expect(preview.getByTestId("batch-preview-column")).toHaveCount(5);
    await expect(preview.locator('[data-testid="batch-preview-column"][data-checked="true"]')).toHaveCount(5);
    await expect(preview.getByTestId("batch-preview-count")).toContainText("128");
    await expect(preview.getByTestId("batch-preview-formatting")).toHaveCount(0);
    await expect(dialog.getByTestId("batch-launch-refusal")).toContainText("128");

    // WHEN the header row is moved to row 2
    await preview.getByTestId("batch-preview-header-row").selectOption("2");

    // THEN the columns are named after that row, and one item fewer is counted
    await expect(preview.getByTestId("batch-preview-column").first()).toContainText("Customer 2");
    await expect(preview.getByTestId("batch-preview-count")).toContainText("127");
    await expect(preview.locator('[data-testid="batch-preview-row"][data-role="header"]')).toContainText("Customer 2");

    // WHEN the Date column is unchecked and the batch launched
    await preview.getByTestId("batch-preview-column").nth(3).getByTestId("batch-preview-column-check").click();
    await expect(preview.getByTestId("batch-preview-column").nth(3)).toHaveAttribute("data-checked", "false");
    await dialog.getByTestId("batch-launch-confirm").click();

    // THEN the launch names the sheet, the header row and the four columns left
    await expect(dialog).toBeHidden();
    const sent = (await launches(page)) as { source: { location: string; options: Record<string, string> } }[];
    expect(sent.at(-1)?.source.location).toBe("/data/requests.xlsx");
    expect(sent.at(-1)?.source.options).toEqual({ sheet: "March", header_row: "2", columns: "A,B,C,E" });
  });

  test("another sheet brings its own preview, and no column checked refuses the launch", async ({ page }) => {
    // GIVEN the preview of a dropped workbook
    const dialog = await openOnDroppedFile(page, "requests.xlsx");
    const preview = dialog.getByTestId("batch-preview");
    await expect(preview.getByTestId("batch-preview-column")).toHaveCount(5);

    // WHEN the other sheet is picked
    await preview.getByTestId("batch-preview-sheet").selectOption("Notes");

    // THEN its single column and its two rows are shown
    await expect(preview.getByTestId("batch-preview-column")).toHaveCount(1);
    await expect(preview.getByTestId("batch-preview-row")).toHaveCount(2);

    // WHEN that column is unchecked
    await preview.getByTestId("batch-preview-column-check").click();

    // THEN the launch is refused: the agent would receive nothing
    await expect(dialog.getByTestId("batch-launch-refusal")).toHaveAttribute("data-refusal", "fields_missing");
    await expect(dialog.getByTestId("batch-launch-confirm")).toBeDisabled();
  });

  test("an .xls workbook says its formatting is not carried into the copy of results", async ({ page }) => {
    // GIVEN an .xls workbook dropped in the chat
    // WHEN "Process each row" is clicked
    const dialog = await openOnDroppedFile(page, "requests.xls");

    // THEN the preview says so, and the original cannot be written
    await expect(dialog.getByTestId("batch-preview-formatting")).toContainText(".xls");
    await expect(dialog.getByTestId("batch-launch-destination-original").getByRole("radio")).toBeDisabled();
  });
});
