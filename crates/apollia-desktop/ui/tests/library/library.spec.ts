import { test, expect, type Page } from "@playwright/test";

/**
 * The Library: the installed packages, what each holds, its publisher and its
 * signature, and the two gestures on a package.
 *
 * Boots the UI bundle with the Tauri bridge stubbed, reaches the Library
 * through its sidebar entry, and reads the list, an open package and the
 * uninstall dialog. The stub holds two packages provided with Apollia, a
 * signed package holding one item of every kind with one automation, and an
 * unsigned package of one agent; it records the uninstallation it receives and
 * takes the package out of its list. The system file chooser answers the path
 * the case names. The bundle boots in the locale of the browser, English here.
 */

type InvokeStub = (cmd: string, args: unknown) => unknown;

interface TauriInternalsStub {
  invoke: InvokeStub;
  transformCallback: (cb: (payload: unknown) => void, once: boolean) => number;
  convertFileSrc: (path: string) => string;
}

async function installTauriStub(page: Page, empty = false): Promise<void> {
  await page.addInitScript((startEmpty: boolean) => {
    const agentRow = (name: string) => ({
      id: `rt-${name}`, name, version: "1.0.0", enabled: true, runtime_status: "active",
      installed_at: "2026-10-01T09:00:00Z", description: `${name} description`, tags: [],
      tools_required: [], tools_optional: [], execution_mode: "direct", install_path: `/agents/${name}`,
      supports_a2a: false, skills: [], agent_type: "assistant", examples: [], limitations: [],
      setup_notes: null, class_name: name, memory_namespace: name, shared_memory_namespaces: [],
    });
    const none = { agents: [], workflows: [], tools: [], connections_brought: [], connections_required: [] };
    const details: Record<string, Record<string, unknown>> = {
      "apollia-guide": {
        name: "apollia-guide", version: "0.3.0", description: "The guide of Apollia.",
        signature: { publisher: "Apollia", signed: false, trusted: false, key_fingerprint: null },
        embedded: true,
        content: { ...none, agents: [{ id: "apollia-guide", name: "Guide", description: "Answers questions about Apollia." }] },
        automations: [],
      },
      "onboarding-agent": {
        name: "onboarding-agent", version: "0.3.0", description: "The first conversation.",
        signature: { publisher: "Apollia", signed: false, trusted: false, key_fingerprint: null },
        embedded: true,
        content: { ...none, agents: [{ id: "onboarding-agent", name: "Faire connaissance", description: "Gets to know you." }] },
        automations: [],
      },
      crm: {
        name: "crm", version: "1.0.0", description: "Qualifies the incoming requests",
        signature: { publisher: "Acme", signed: true, trusted: true, key_fingerprint: "0123456789abcdef0123456789abcdef" },
        embedded: false,
        content: {
          agents: [{ id: "crm-agent", name: "Qualifier", description: "Qualify the requests." }],
          workflows: [{ id: "weekly", name: "weekly", description: "Weekly digest." }],
          tools: [{ name: "lookup", description: "Look up a lead.", family: "read" }],
          connections_brought: ["ledger"],
          connections_required: [{ type: "gmail", reason: "Read the requests" }],
        },
        automations: [{ id: "crm-daily", target: "crm-agent" }],
      },
      veille: {
        name: "veille", version: "0.4.0", description: "Morning watch",
        signature: { publisher: "Ada", signed: false, trusted: false, key_fingerprint: null },
        embedded: false,
        content: { ...none, agents: [{ id: "veille-agent", name: "Veille", description: "Watches the news." }] },
        automations: [],
      },
    };
    let installed = startEmpty ? [] : Object.keys(details);
    const listItem = (name: string) => {
      const d = details[name] as {
        content: { agents: unknown[]; workflows: unknown[]; tools: unknown[]; connections_brought: unknown[]; connections_required: unknown[] };
      };
      return {
        ...details[name],
        agent_count: d.content.agents.length, agents: [], installed_at: "2026-10-01T08:00:00Z",
        root_path: `/data/agents/packages/${name}`, root_missing: false,
        counts: {
          agents: d.content.agents.length, workflows: d.content.workflows.length, tools: d.content.tools.length,
          connections: d.content.connections_brought.length + d.content.connections_required.length,
        },
      };
    };
    const updatePreview = {
      name: "crm", version: "1.1.0", description: "Qualifies the incoming requests", sdk: ">=0.3",
      signature: {
        status: "signed", publisher: "Acme", declaredPublisher: "Acme",
        keyFingerprint: "0123456789abcdef0123456789abcdef", publicKey: "ab".repeat(32), trusted: true, reason: null,
      },
      signatureRequired: false, agents: [], workflows: [], tools: [], requires: [], mcpServers: [], pip: [],
      triggers: [], installedVersion: "1.0.0",
      update: { fromVersion: "1.0.0", toVersion: "1.1.0", downgrade: false, addedAccess: [], addedRequires: [], addedMcpServers: [] },
      notices: [], refusal: null,
    };
    const calls: { cmd: string; args: unknown }[] = [];
    (globalThis as unknown as { __libraryCalls: typeof calls }).__libraryCalls = calls;
    const handlers: Record<string, (args: unknown) => unknown> = {
      get_onboarding_state: () => ({ completed: true, skipped: false, started_at: null }),
      get_config: () => ({ config_path: "~/.apollia/apollia.toml", config_exists: true, sections: [] }),
      get_system_info: () => ({}),
      list_agents: () => [agentRow("invoices"), agentRow("crm-agent")],
      list_agent_packages: () => installed.map(listItem),
      get_agent_package_detail: (args) => {
        const name = (args as { name: string }).name;
        if (!installed.includes(name)) throw new Error(`package '${name}' not found`);
        return {
          ...details[name], author: "", agents: [], installed_at: "2026-10-01T08:00:00Z",
          updated_at: "2026-10-01T08:00:00Z", root_path: `/data/agents/packages/${name}`,
          root_missing: false, manifest: {},
        };
      },
      uninstall_agent_package: (args) => {
        calls.push({ cmd: "uninstall_agent_package", args });
        const name = (args as { name: string }).name;
        installed = installed.filter((n) => n !== name);
        return {
          name, version: "1.0.0", agents: ["crm-agent"], workflows: ["weekly"], tools: ["lookup"],
          triggersRemoved: ["crm-daily"], mcpServersRemoved: ["ledger"], connectionsKept: ["gmail"],
          memoryCleared: false, historyCleared: false, leftovers: [],
        };
      },
      "plugin:dialog|open": () => (globalThis as unknown as { __nextFile: string }).__nextFile,
      preview_package_file: () => updatePreview,
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
  }, empty);
}

async function openLibrary(page: Page): Promise<void> {
  await page.goto("/");
  await page.getByTestId("nav-library").first().click();
  await expect(page.getByTestId("library-page")).toBeVisible();
}

function row(page: Page, name: string) {
  return page.locator(`[data-testid="library-package-row"][data-package="${name}"]`);
}

async function calls(page: Page): Promise<{ cmd: string; args: Record<string, unknown> }[]> {
  return page.evaluate(
    () => (globalThis as unknown as { __libraryCalls: { cmd: string; args: Record<string, unknown> }[] }).__libraryCalls,
  );
}

test.describe("library", () => {
  test("the list names each package, its publisher, its version, its content and its signature", async ({ page }) => {
    // GIVEN two packages provided with Apollia, a signed one and an unsigned one
    await installTauriStub(page);

    // WHEN the Library is opened from the navigation
    await openLibrary(page);

    // THEN the provided packages are one entry, first, and each package is a line
    const rows = page.getByTestId("library-package-row");
    await expect(rows).toHaveCount(3);
    await expect(rows.nth(0)).toHaveAttribute("data-package", "@provided");
    await expect(rows.nth(0).getByTestId("library-package-content")).toContainText("2 agents");
    // AND a line carries the publisher, the version, the content and the signature
    const crm = row(page, "crm");
    await expect(crm.getByTestId("library-package-name")).toHaveText("crm");
    await expect(crm.getByTestId("library-package-origin")).toContainText("Acme");
    await expect(crm.getByTestId("library-package-origin")).toContainText("1.0.0");
    await expect(crm.getByTestId("library-package-content")).toContainText("1 agent");
    await expect(crm.getByTestId("library-package-content")).toContainText("2 connections");
    await expect(crm).toHaveAttribute("data-signed", "true");
    await expect(row(page, "veille")).toHaveAttribute("data-signed", "false");
    // AND the tally counts the entries, signed and not
    await expect(page.getByTestId("library-tally")).toContainText("3 packages");
    await expect(page.getByTestId("library-tally")).toContainText("2 signed");

    // WHEN the list is filtered by a publisher
    await page.getByTestId("library-filter").fill("ada");

    // THEN only its package stays
    await expect(rows).toHaveCount(1);
    await expect(rows.first()).toHaveAttribute("data-package", "veille");
  });

  test("an open package shows its content by kind, and an agent leads to its sheet", async ({ page }) => {
    // GIVEN the Library, and the signed package opened
    await installTauriStub(page);
    await openLibrary(page);
    await row(page, "crm").click();
    const detail = page.getByTestId("library-detail");

    // WHEN its content is read
    // THEN each kind lists its items, the tool with its family, the
    // connections as brought or required, and the signature names the publisher
    await expect(page.getByTestId("library-detail-title")).toHaveText("crm");
    await expect(page.getByTestId("library-detail-signature")).toHaveAttribute("data-state", "signed");
    await expect(page.getByTestId("library-detail-signature")).toContainText("Acme");
    await expect(page.getByTestId("library-content-agent")).toContainText("Qualifier");
    await expect(page.getByTestId("library-content-workflow")).toContainText("weekly");
    await expect(page.getByTestId("library-content-tool")).toContainText("lookup");
    await expect(page.getByTestId("library-content-tool").getByTestId("library-content-tag")).toHaveText("Read");
    await expect(page.getByTestId("library-content-connection")).toContainText("ledger");
    await expect(page.getByTestId("library-content-connection")).toContainText("gmail");
    await expect(detail.getByTestId("library-detail-missing")).toHaveCount(0);

    // WHEN the sheet of its agent is opened
    await page.locator('[data-testid="library-open-sheet"][data-item="crm-agent"]').click();

    // THEN the Agents screen shows that agent, not the first of its list
    await expect(page.getByTestId("agents-page")).toBeVisible();
    await expect(page.getByTestId("agent-detail-title")).toHaveText("crm-agent");
  });

  test("the packages provided with Apollia open without update nor uninstall", async ({ page }) => {
    // GIVEN the Library
    await installTauriStub(page);
    await openLibrary(page);

    // WHEN the provided entry is opened
    await row(page, "@provided").click();

    // THEN both agents are listed, the entry is marked, and neither gesture is offered
    await expect(page.getByTestId("library-detail-provided")).toBeVisible();
    await expect(page.getByTestId("library-content-agent")).toContainText("Guide");
    await expect(page.getByTestId("library-content-agent")).toContainText("Faire connaissance");
    await expect(page.getByTestId("library-detail-missing")).toContainText("workflow, tool or connection");
    await expect(page.getByTestId("library-update-button")).toHaveCount(0);
    await expect(page.getByTestId("library-uninstall-button")).toHaveCount(0);

    // WHEN the user goes back
    await page.getByTestId("library-detail-back").click();

    // THEN the list is shown again
    await expect(page.getByTestId("library-list")).toBeVisible();
  });

  test("updating chooses a package file and opens its preview as an update", async ({ page }) => {
    // GIVEN the signed package opened, and a more recent file to choose
    await installTauriStub(page);
    await openLibrary(page);
    await row(page, "crm").click();
    await page.evaluate(() => {
      (globalThis as unknown as { __nextFile: string }).__nextFile = "/files/crm-1.1.0.apo";
    });

    // WHEN "Update" is pressed
    await page.getByTestId("library-update-button").click();

    // THEN the preview dialog opens on that file, in its update form
    const dialog = page.getByTestId("package-preview-dialog");
    await expect(dialog).toBeVisible();
    await expect(dialog.locator('[data-variant="update"]')).toBeVisible();
    await expect(page.getByTestId("package-preview-origin")).toContainText("crm-1.1.0.apo");
    await expect(page.getByTestId("package-preview-origin")).toContainText("Update");
  });

  test("uninstalling says what goes and what stays, keeps the data by default, and removes the package", async ({ page }) => {
    // GIVEN the signed package opened
    await installTauriStub(page);
    await openLibrary(page);
    await row(page, "crm").click();

    // WHEN "Uninstall" is pressed
    await page.getByTestId("library-uninstall-button").click();

    // THEN the dialog lists what goes, the automation and the brought
    // connection included, and what stays
    const dialog = page.getByTestId("library-uninstall-dialog");
    await expect(dialog).toBeVisible();
    await expect(page.getByTestId("library-uninstall-title")).toContainText("crm");
    await expect(page.getByTestId("library-uninstall-leaving-agents")).toContainText("Qualifier");
    await expect(page.getByTestId("library-uninstall-leaving-workflows")).toContainText("weekly");
    await expect(page.getByTestId("library-uninstall-leaving-tools")).toContainText("lookup");
    await expect(page.getByTestId("library-uninstall-leaving-automations")).toContainText("crm-daily");
    await expect(page.getByTestId("library-uninstall-leaving-connections")).toContainText("ledger");
    await expect(page.getByTestId("library-uninstall-staying")).toContainText("gmail");
    // AND keeping is the default, and the action says so
    await expect(page.getByTestId("library-uninstall-confirm")).toHaveText("Uninstall");

    // WHEN the user chooses to erase
    await page.getByTestId("library-uninstall-erase").click();

    // THEN the action names it
    await expect(page.getByTestId("library-uninstall-confirm")).toHaveText("Uninstall and erase");

    // WHEN the user confirms
    await page.getByTestId("library-uninstall-confirm").click();

    // THEN the uninstallation is sent with the choice, and the package leaves the list
    await expect(dialog).toBeHidden();
    await expect(page.getByTestId("library-list")).toBeVisible();
    await expect(row(page, "crm")).toHaveCount(0);
    expect(await calls(page)).toEqual([
      { cmd: "uninstall_agent_package", args: { name: "crm", clearMemory: true, clearHistory: true } },
    ]);
  });

  test("keeping the data sends an uninstallation that erases nothing", async ({ page }) => {
    // GIVEN the uninstall dialog of the signed package, left on its default
    await installTauriStub(page);
    await openLibrary(page);
    await row(page, "crm").click();
    await page.getByTestId("library-uninstall-button").click();

    // WHEN the user confirms
    await page.getByTestId("library-uninstall-confirm").click();

    // THEN the memory and the history are kept
    await expect(page.getByTestId("library-uninstall-dialog")).toBeHidden();
    expect(await calls(page)).toEqual([
      { cmd: "uninstall_agent_package", args: { name: "crm", clearMemory: false, clearHistory: false } },
    ]);
  });

  test("with no package installed the Library says so and offers to install one", async ({ page }) => {
    // GIVEN no installed package
    await installTauriStub(page, true);

    // WHEN the Library is opened
    await openLibrary(page);

    // THEN the empty state shows, with its two gestures
    await expect(page.getByTestId("library-empty")).toBeVisible();
    await expect(page.getByTestId("library-empty-install")).toBeVisible();
    await expect(page.getByTestId("library-empty-examples")).toBeVisible();
    await expect(page.getByTestId("library-package-row")).toHaveCount(0);
  });
});
