import { test, expect, type Page } from "@playwright/test";

/**
 * "Try with the example" on the example that comes with a demonstration
 * dataset.
 *
 * Boots the UI bundle with the Tauri bridge stubbed, reaches the Agents
 * screen and plays the gesture: on an example not installed, the preview of
 * the package comes first; the copy dialog follows its installation, and
 * copies only on confirmation, into the folder shown. Once copied, the folder
 * of reference documents is proposed as a source, checked, and the workbook
 * as a batch, whose launch dialog opens on it. The stub records every call
 * that writes.
 */

type InvokeStub = (cmd: string, args: unknown) => unknown;

interface TauriInternalsStub {
  invoke: InvokeStub;
  transformCallback: (cb: (payload: unknown) => void, once: boolean) => number;
  convertFileSrc: (path: string) => string;
}

const EXAMPLE = "qualify-requests";
const DESTINATION = "/Users/demo/Documents/Apollia, example";

async function installTauriStub(page: Page): Promise<void> {
  await page.addInitScript((example: string) => {
    let installed = false;
    let sources: string[] = [];
    const calls: { cmd: string; args: unknown }[] = [];
    (globalThis as unknown as { __trialCalls: typeof calls }).__trialCalls = calls;
    const record = (cmd: string, args: unknown) => calls.push({ cmd, args });
    const handlers: Record<string, (args: unknown) => unknown> = {
      get_onboarding_state: () => ({ completed: true, skipped: false, started_at: null }),
      read_config_file: () => ({ path: "~/.apollia/apollia.toml", exists: true, text: "", sha256: "", diagnostics: [], locked: [], pendingRestart: [] }),
      get_system_info: () => ({}),
      list_example_packages: () => [
        {
          name: example,
          version: "1.0.0",
          description: "Qualifies a batch of requests.",
          agents: [{ id: example, name: "Qualify requests", mission: "Qualifies each request of a batch, with its sources." }],
          requires: [],
          installedVersion: installed ? "1.0.0" : null,
        },
        {
          name: "veille-web",
          version: "1.0.0",
          description: "Watches a topic on the web.",
          agents: [{ id: "veille-web", name: "Veille web", mission: "Searches the web on your topic." }],
          requires: [],
          installedVersion: null,
        },
      ],
      preview_example_package: (args) => {
        record("preview_example_package", args);
        return {
          name: example,
          version: "1.0.0",
          description: "Qualifies a batch of requests.",
          sdk: ">=0.3",
          signature: { status: "unsigned", publisher: "Apollia", declaredPublisher: "Apollia", trusted: false },
          signatureRequired: false,
          agents: [{ id: example, name: "Qualify requests", mission: "Qualifies each request.", examples: [], access: [] }],
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
        };
      },
      install_example_package: (args) => {
        record("install_example_package", args);
        installed = true;
        return { name: example, version: "1.0.0" };
      },
      "plugin:path|resolve_directory": () => "/Users/demo/Documents",
      "plugin:path|join": (args) => (args as { paths: string[] }).paths.join("/"),
      copy_example_dataset: (args) => {
        record("copy_example_dataset", args);
        const destination = (args as { destination: string }).destination;
        return {
          destination,
          workbook: `${destination}/demandes.xlsx`,
          folder: `${destination}/referentiel`,
          rows: 12,
          kept: [],
        };
      },
      list_connections: () => [],
      add_folder_connection: (args) => {
        record("add_folder_connection", args);
        return {
          id: "folder:ref",
          kind: "folder",
          label: "referentiel",
          account: null,
          path: (args as { path: string }).path,
          provenance: "user",
          remote: false,
          official: false,
          state: "connected",
          tools: [],
        };
      },
      list_sources: () => [],
      add_source: (args) => {
        record("add_source", args);
        const wanted = (args as { source: { kind: string; label: string; reference: string } }).source;
        return { id: "source:ref", group: null, indexState: "absent", addedAt: "2026-10-06T00:00:00Z", ...wanted };
      },
      get_agent_definition: () => ({
        id: example,
        origin: "package",
        package: example,
        version: "1.0.0",
        enabled: true,
        name: "Qualify requests",
        mission: "",
        examples: [],
        access: [],
        sources,
        model: null,
        autonomy: "assisted",
        profile: null,
        overridden: [],
      }),
      set_agent_override: (args) => {
        record("set_agent_override", args);
        sources = (args as { value: string[] }).value;
        return null;
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
  }, EXAMPLE);
}

async function openAgents(page: Page): Promise<void> {
  await page.goto("/");
  await page.getByTestId("nav-agents").first().click();
  await expect(page.getByTestId("agents-page")).toBeVisible();
  await expect(page.getByTestId("agents-examples")).toBeVisible();
}

async function calls(page: Page): Promise<{ cmd: string; args: Record<string, unknown> }[]> {
  return page.evaluate(
    () => (globalThis as unknown as { __trialCalls: { cmd: string; args: Record<string, unknown> }[] }).__trialCalls,
  );
}

function card(page: Page, name: string) {
  return page.locator(`[data-testid="agents-example-card"][data-example="${name}"]`);
}

test.describe("try with the example", () => {
  test("only the example with a dataset offers the trial, and showing it copies nothing", async ({ page }) => {
    // GIVEN a fresh application
    await installTauriStub(page);

    // WHEN the Agents screen is opened on its examples
    await openAgents(page);

    // THEN the trial is offered on "qualify requests" and on no other card
    await expect(card(page, EXAMPLE).getByTestId("agents-example-try")).toBeVisible();
    await expect(card(page, "veille-web").getByTestId("agents-example-try")).toHaveCount(0);
    // AND nothing was previewed, installed or copied
    expect(await calls(page)).toEqual([]);
  });

  test("on an example not installed, the preview comes first, then the copy on confirmation", async ({ page }) => {
    // GIVEN the examples of a fresh application
    await installTauriStub(page);
    await openAgents(page);

    // WHEN the user tries the example
    await card(page, EXAMPLE).getByTestId("agents-example-try").click();

    // THEN the preview of the package opens first, and no copy dialog
    await expect(page.getByTestId("package-preview-dialog")).toBeVisible();
    await expect(page.getByTestId("example-trial-dialog")).toHaveCount(0);
    expect((await calls(page)).map((call) => call.cmd)).toEqual(["preview_example_package"]);

    // WHEN the user installs it from the preview
    await page.getByTestId("package-preview-install").click();

    // THEN the copy dialog opens on the proposed folder, says the data is
    // made up, and nothing is copied yet
    await expect(page.getByTestId("package-preview-dialog")).toBeHidden();
    await expect(page.getByTestId("example-trial-title")).toBeVisible();
    await expect(page.getByTestId("example-trial-fictional")).toBeVisible();
    await expect(page.getByTestId("example-trial-destination")).toHaveValue(DESTINATION);
    expect((await calls(page)).map((call) => call.cmd)).toEqual([
      "preview_example_package",
      "install_example_package",
    ]);

    // WHEN the user confirms the copy
    await page.getByTestId("example-trial-copy").click();

    // THEN the dataset is copied into that folder, and the dialog says where
    // and proposes the folder as a source, checked
    await expect(page.getByTestId("example-trial-copied")).toBeVisible();
    await expect(page.getByTestId("example-trial-where")).toContainText(DESTINATION);
    await expect(page.getByTestId("example-trial-also-source")).toBeChecked();
    expect((await calls(page)).at(-1)).toEqual({
      cmd: "copy_example_dataset",
      args: { example: EXAMPLE, destination: DESTINATION },
    });

    // WHEN the user launches the batch on the workbook
    await page.getByTestId("example-trial-launch").click();

    // THEN the folder of reference documents became a source checked for the
    // agent, and the launch dialog of a batch opens on the copied workbook
    await expect(page.getByTestId("batch-launch-dialog")).toBeVisible();
    await expect(page.getByTestId("batch-launch-dialog")).toContainText("demandes.xlsx");
    await expect(page.getByTestId("example-trial-dialog")).toHaveCount(0);
    expect((await calls(page)).slice(3)).toEqual([
      { cmd: "add_folder_connection", args: { path: `${DESTINATION}/referentiel` } },
      { cmd: "add_source", args: { source: { kind: "folder", label: "referentiel", reference: "folder:ref" } } },
      { cmd: "set_agent_override", args: { name: EXAMPLE, field: "sources", value: ["source:ref"] } },
    ]);
  });

  test("a preview or a copy dialog left without confirming copies nothing", async ({ page }) => {
    // GIVEN the trial asked for on an example not installed
    await installTauriStub(page);
    await openAgents(page);
    await card(page, EXAMPLE).getByTestId("agents-example-try").click();

    // WHEN the user cancels the preview
    await page.getByTestId("package-preview-cancel").click();

    // THEN nothing is installed and no copy dialog opens
    await expect(page.getByTestId("package-preview-dialog")).toBeHidden();
    await expect(page.getByTestId("example-trial-dialog")).toHaveCount(0);

    // WHEN the example is installed on its own, then tried, and the copy
    // dialog is cancelled
    await card(page, EXAMPLE).getByTestId("agents-example-install").click();
    await page.getByTestId("package-preview-install").click();
    await expect(card(page, EXAMPLE)).toHaveAttribute("data-installed", "true");
    await expect(page.getByTestId("example-trial-dialog")).toHaveCount(0);
    await card(page, EXAMPLE).getByTestId("agents-example-try").click();
    await expect(page.getByTestId("example-trial-title")).toBeVisible();
    await page.getByTestId("example-trial-cancel").click();

    // THEN the dialog closes and nothing was copied
    await expect(page.getByTestId("example-trial-title")).toBeHidden();
    expect((await calls(page)).map((call) => call.cmd)).not.toContain("copy_example_dataset");
  });
});
