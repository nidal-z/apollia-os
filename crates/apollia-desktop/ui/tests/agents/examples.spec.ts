import { test, expect, type Page } from "@playwright/test";

/**
 * The examples the Agents screen proposes.
 *
 * Boots the UI bundle with the Tauri bridge stubbed, reaches the Agents
 * screen and reads its Examples section: a band beside agents and workflows,
 * open cards on a screen with nothing else. "Install" opens the preview every
 * package gets, with no warning about a signature, and installs the example
 * by its name; an installed example says so and leads to the sheet of its
 * agent. The stub records the installation it receives.
 */

type InvokeStub = (cmd: string, args: unknown) => unknown;

interface TauriInternalsStub {
  invoke: InvokeStub;
  transformCallback: (cb: (payload: unknown) => void, once: boolean) => number;
  convertFileSrc: (path: string) => string;
}

interface StubOptions {
  /** The screen already lists a workflow. */
  withWorkflow: boolean;
}

async function installTauriStub(page: Page, options: StubOptions): Promise<void> {
  await page.addInitScript((opts: StubOptions) => {
    const installed = new Set<string>();
    const examples = () => [
      {
        name: "veille-web",
        version: "1.0.0",
        description: "Watches a topic on the web.",
        agents: [{ id: "veille-web", name: "Veille web", mission: "Searches the web on your topic and writes a summary." }],
        requires: [],
        installedVersion: installed.has("veille-web") ? "1.0.0" : null,
      },
      {
        name: "tri-de-mails",
        version: "1.0.0",
        description: "Sorts your mails into a table.",
        agents: [{ id: "tri-de-mails", name: "Tri de mails", mission: "Sorts your mails by category and urgency. Never changes your mailbox." }],
        requires: [{ type: "imap", reason: "Reads your mails", present: false, broughtByPackage: false }],
        installedVersion: installed.has("tri-de-mails") ? "1.0.0" : null,
      },
    ];
    const preview = (name: string) => ({
      name,
      version: "1.0.0",
      description: "Watches a topic on the web.",
      sdk: ">=0.3",
      signature: { status: "unsigned", publisher: "Apollia", declaredPublisher: "Apollia", trusted: false },
      signatureRequired: true,
      agents: [{ id: name, name: "Veille web", mission: "Searches the web on your topic.", examples: [], access: [] }],
      workflows: [],
      tools: [],
      requires: [],
      mcpServers: [],
      pip: [],
      triggers: [],
      installedVersion: null,
      update: null,
      notices: [],
      refusal: null,
    });
    const calls: { cmd: string; args: unknown }[] = [];
    (globalThis as unknown as { __exampleCalls: typeof calls }).__exampleCalls = calls;
    const handlers: Record<string, (args: unknown) => unknown> = {
      get_onboarding_state: () => ({ completed: true, skipped: false, started_at: null }),
      read_config_file: () => ({ path: "~/.apollia/apollia.toml", exists: true, text: "", sha256: "", diagnostics: [], locked: [], pendingRestart: [] }),
      get_system_info: () => ({}),
      list_workflows: () =>
        opts.withWorkflow
          ? [{ workflowId: "sales/weekly", name: "weekly", description: "Weekly report" }]
          : [],
      list_example_packages: () => examples(),
      preview_example_package: (args) => {
        calls.push({ cmd: "preview_example_package", args });
        return preview((args as { name: string }).name);
      },
      install_example_package: (args) => {
        calls.push({ cmd: "install_example_package", args });
        const name = (args as { name: string }).name;
        installed.add(name);
        return { name, version: "1.0.0" };
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
            return Promise.reject(String(err));
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
  }, options);
}

async function openAgents(page: Page): Promise<void> {
  await page.goto("/");
  await page.getByTestId("nav-agents").first().click();
  await expect(page.getByTestId("agents-page")).toBeVisible();
  await expect(page.getByTestId("agents-examples")).toBeVisible();
}

async function calls(page: Page): Promise<{ cmd: string; args: Record<string, unknown> }[]> {
  return page.evaluate(
    () => (globalThis as unknown as { __exampleCalls: { cmd: string; args: Record<string, unknown> }[] }).__exampleCalls,
  );
}

test.describe("agent examples", () => {
  test("a screen with nothing else opens on the examples, none of them installed", async ({ page }) => {
    // GIVEN a fresh application: no agent, no workflow
    await installTauriStub(page, { withWorkflow: false });

    // WHEN the Agents screen is opened
    await openAgents(page);

    // THEN the Examples section is open on one card per example, each with
    // its name, its mission and what it requires, and none reads installed
    await expect(page.getByTestId("agents-examples")).toHaveAttribute("data-open", "true");
    const cards = page.getByTestId("agents-example-card");
    await expect(cards).toHaveCount(2);
    await expect(cards.nth(0)).toContainText("Veille web");
    await expect(cards.nth(0)).toContainText("Searches the web on your topic");
    await expect(cards.nth(1).getByTestId("agents-example-requires")).toContainText("imap");
    await expect(page.getByTestId("agents-example-installed")).toHaveCount(0);
    await expect(page.getByTestId("agents-example-install")).toHaveCount(2);
    // AND nothing was previewed or installed by opening the screen
    expect(await calls(page)).toEqual([]);
  });

  test("beside a workflow the examples are a band that opens on request", async ({ page }) => {
    // GIVEN a screen that already lists a workflow
    await installTauriStub(page, { withWorkflow: true });
    await openAgents(page);

    // WHEN the section is read
    // THEN it is a band that names the examples, with no card yet
    await expect(page.getByTestId("agents-examples")).toHaveAttribute("data-open", "false");
    await expect(page.getByTestId("agents-examples-names")).toContainText("Veille web, Tri de mails");
    await expect(page.getByTestId("agents-example-card")).toHaveCount(0);

    // WHEN the user asks to see the examples
    await page.getByTestId("agents-examples-show").click();

    // THEN the cards show
    await expect(page.getByTestId("agents-example-card")).toHaveCount(2);
  });

  test("Install opens the preview, with no signature warning, then installs the example", async ({ page }) => {
    // GIVEN the examples open on a fresh application
    await installTauriStub(page, { withWorkflow: false });
    await openAgents(page);

    // WHEN the user chooses to install the first example
    await page.getByTestId("agents-example-card").nth(0).getByTestId("agents-example-install").click();

    // THEN the preview of a package opens on it, says where it comes from,
    // and neither warns about its signature nor refuses it
    const dialog = page.getByTestId("package-preview-dialog");
    await expect(dialog).toBeVisible();
    await expect(page.getByTestId("package-preview-title")).toContainText("veille-web");
    await expect(page.getByTestId("package-preview-publisher")).toHaveAttribute("data-state", "shipped");
    await expect(page.getByTestId("package-preview-unsigned-warning")).toHaveCount(0);
    // AND nothing is installed yet
    expect((await calls(page)).map((call) => call.cmd)).toEqual(["preview_example_package"]);

    // WHEN the user installs it from the preview
    await page.getByTestId("package-preview-install").click();

    // THEN the installation names the example, the dialog closes, and the
    // card reads installed with the way to its sheet
    await expect(dialog).toBeHidden();
    expect(await calls(page)).toEqual([
      { cmd: "preview_example_package", args: { name: "veille-web" } },
      { cmd: "install_example_package", args: { name: "veille-web", depsConfirmed: false, changesConfirmed: false } },
    ]);
    const card = page.locator('[data-testid="agents-example-card"][data-example="veille-web"]');
    await expect(card).toHaveAttribute("data-installed", "true");
    await expect(card.getByTestId("agents-example-installed")).toBeVisible();
    await expect(card.getByTestId("agents-example-open-sheet")).toBeVisible();
    await expect(card.getByTestId("agents-example-install")).toHaveCount(0);
    // AND the other example is still only proposed
    await expect(
      page.locator('[data-testid="agents-example-card"][data-example="tri-de-mails"]'),
    ).toHaveAttribute("data-installed", "false");
  });

  test("cancelling the preview installs nothing", async ({ page }) => {
    // GIVEN the preview of an example
    await installTauriStub(page, { withWorkflow: false });
    await openAgents(page);
    await page.getByTestId("agents-example-card").nth(0).getByTestId("agents-example-install").click();
    await expect(page.getByTestId("package-preview-dialog")).toBeVisible();

    // WHEN the user cancels
    await page.getByTestId("package-preview-cancel").click();

    // THEN no installation was sent and the example is still proposed
    await expect(page.getByTestId("package-preview-dialog")).toBeHidden();
    expect((await calls(page)).map((call) => call.cmd)).toEqual(["preview_example_package"]);
    await expect(page.getByTestId("agents-example-installed")).toHaveCount(0);
  });
});
