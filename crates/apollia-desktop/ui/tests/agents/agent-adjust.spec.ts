import { test, expect, type Page } from "@playwright/test";

/**
 * Adjusting an agent on its sheet, and going back to its defaults.
 *
 * Boots the UI bundle with the Tauri bridge stubbed, as
 * `tests/agents/agent-sheet.spec.ts` does, and adjusts the built-in Apollia
 * agent: its mission changed, another agent granted, the profile unchecked,
 * saved; then sent back to its defaults after the confirmation. A connection
 * of the selection is granted too: its reading tools come checked, the others
 * not, and the remote connection is marked on the sheet. The stub keeps the
 * agent in force as the runtime would, and records the writes it receives.
 */

type InvokeStub = (cmd: string, args: unknown) => unknown;

interface TauriInternalsStub {
  invoke: InvokeStub;
  transformCallback: (cb: (payload: unknown) => void, once: boolean) => number;
  convertFileSrc: (path: string) => string;
}

interface Recorded {
  __writes: { cmd: string; args: unknown }[];
}

async function installTauriStub(page: Page): Promise<void> {
  await page.addInitScript(() => {
    const defaults = {
      mission: "",
      examples: [] as string[],
      access: ["file_read"],
      sources: ["profile"],
    };
    let apollia = {
      id: "apollia:chat", origin: "interface", package: null, version: "1", enabled: true,
      name: "Apollia", ...defaults, model: null as unknown, autonomy: "supervised", profile: null,
      overridden: [] as string[],
      definition: { id: "apollia:chat", name: "Apollia", mission: "", defaults: {} },
    };
    const writes: { cmd: string; args: unknown }[] = [];
    (globalThis as unknown as Recorded).__writes = writes;
    const handlers: Record<string, (args: unknown) => unknown> = {
      get_onboarding_state: () => ({ completed: true, skipped: false, started_at: null }),
      get_config: () => ({ config_path: "~/.apollia/apollia.toml", config_exists: true, sections: [] }),
      get_system_info: () => ({}),
      get_agent_definition: () => apollia,
      describe_tool: (args) =>
        (args as { name: string }).name === "file_read"
          ? {
              name: "file_read", version: "1", description: "Reads the files of the folder you chose",
              kind: { type: "native" }, input_schema: null, output_schema: null, permissions: [],
              family: "read", family_source: "builtin",
            }
          : null,
      list_tools: () => [{ name: "file_read", version: "1", description: "Reads the files", kind: "native" }],
      list_mcp_exposable: () => [
        { kind: "agent", item_id: "veille", name: "Veille", description: "Watches the web", exposed: false },
        { kind: "workflow", item_id: "billing/close-month", name: "Close month", description: "Closes the month", exposed: false },
      ],
      list_model_presets: () => [],
      list_connections: () => [
        {
          id: "mcp:outlook", kind: "mcp", label: "Outlook", account: "support@example.com",
          provenance: "selection", remote: true, official: true, state: "connected",
          tools: [
            { id: "mcp:outlook/read_mail", family: "read", familySource: "selection", prechecked: true },
            { id: "mcp:outlook/search_mail", family: "read", familySource: "selection", prechecked: true },
            { id: "mcp:outlook/move_mail", family: "write_or_delete", familySource: "selection", prechecked: false },
            { id: "mcp:outlook/send_mail", family: "send_external", familySource: "selection", prechecked: false },
            { id: "mcp:outlook/export_box", family: "unclassified", familySource: "none", prechecked: false },
          ],
        },
      ],
      set_agent_override: (args) => {
        const { field, value } = args as { field: string; value: unknown };
        writes.push({ cmd: "set_agent_override", args });
        apollia = { ...apollia, [field]: value, overridden: [...new Set([...apollia.overridden, field])] };
        return apollia;
      },
      reset_agent_overrides: (args) => {
        writes.push({ cmd: "reset_agent_overrides", args });
        apollia = { ...apollia, ...defaults, model: null, overridden: [] };
        return apollia;
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

async function openApollia(page: Page): Promise<void> {
  await page.goto("/");
  await page.getByTestId("nav-agents").first().click();
  await expect(page.getByTestId("agents-page")).toBeVisible();
  await page.getByTestId("apollia-chat-pinned").click();
  await expect(page.getByTestId("agent-sheet")).toHaveAttribute("data-agent", "apollia:chat");
}

async function writes(page: Page): Promise<{ cmd: string; args: unknown }[]> {
  return page.evaluate(() => (globalThis as unknown as Recorded).__writes);
}

test.describe("agent adjustment", () => {
  test.beforeEach(async ({ page }) => {
    await installTauriStub(page);
  });

  test("the Apollia agent is adjusted on its sheet, then sent back to its defaults", async ({ page }) => {
    // GIVEN the sheet of the Apollia agent, opened for adjustment
    await openApollia(page);
    await page.getByTestId("agent-adjust-open").click();
    await expect(page.getByTestId("agent-adjust-mission")).toBeVisible();

    // WHEN its mission is changed, the agent Veille granted, the profile unchecked, and the sheet saved
    await page.getByTestId("agent-adjust-mission").fill("Answer in French.");
    await page.getByTestId("agent-adjust-grant").click();
    const dialog = page.getByTestId("agent-adjust-grant-dialog");
    await dialog
      .locator('[data-testid="agent-adjust-grant-row"][data-access="agent:veille"]')
      .getByTestId("agent-adjust-grant-check")
      .click();
    await page.getByTestId("agent-adjust-grant-confirm").click();
    await page
      .locator('[data-testid="agent-adjust-source"][data-source="profile"]')
      .getByTestId("agent-adjust-source-check")
      .click();
    await expect(page.getByTestId("agent-adjust-pending")).toContainText("3");
    await page.getByTestId("agent-adjust-save").click();

    // THEN one override per field was written, the agent granted as agent:veille
    await expect(page.getByTestId("agent-adjust-save")).toHaveCount(0);
    expect(await writes(page)).toEqual([
      { cmd: "set_agent_override", args: { name: "apollia:chat", field: "mission", value: "Answer in French." } },
      { cmd: "set_agent_override", args: { name: "apollia:chat", field: "access", value: ["file_read", "agent:veille"] } },
      { cmd: "set_agent_override", args: { name: "apollia:chat", field: "sources", value: [] } },
    ]);
    // AND the sheet reads the mission in force, marked as the user's
    await expect(page.getByTestId("agent-sheet-mission-text")).toHaveText("Answer in French.");
    await expect(page.getByTestId("agent-sheet-mission-modified")).toBeVisible();

    // WHEN the agent is sent back to its defaults and the confirmation accepted
    await page.getByTestId("agent-adjust-open").click();
    await page.getByTestId("agent-adjust-reset").click();
    await page.getByTestId("agent-adjust-reset-dialog-confirm").click();

    // THEN every override is removed in one call and no section is marked any more
    await expect(page.getByTestId("agent-sheet-mission-modified")).toHaveCount(0);
    expect((await writes(page)).at(-1)).toEqual({ cmd: "reset_agent_overrides", args: { name: "apollia:chat" } });
    await expect(page.getByTestId("agent-sheet-mission-empty")).toBeVisible();
  });

  test("granting a connection of the selection checks its reading tools only", async ({ page }) => {
    // GIVEN the sheet of the Apollia agent in adjustment, and a remote Outlook connection of the selection
    await openApollia(page);
    await page.getByTestId("agent-adjust-open").click();

    // WHEN the connection is chosen in the connection dialog
    await page.getByTestId("agent-adjust-grant-connection").click();
    const dialog = page.getByTestId("agent-adjust-connection-dialog");
    await dialog.locator('[data-testid="agent-adjust-connection-choice"][data-connection="mcp:outlook"]').click();

    // THEN its tools are listed by family, the reading ones checked and the others not
    await expect(dialog.getByTestId("agent-adjust-connection-remote")).toBeVisible();
    await expect(dialog.getByTestId("agent-adjust-connection-family")).toHaveCount(6);
    const check = (id: string) =>
      dialog
        .locator(`[data-testid="agent-adjust-connection-tool"][data-access="${id}"]`)
        .getByTestId("agent-adjust-connection-check");
    await expect(check("mcp:outlook/read_mail")).toBeChecked();
    await expect(check("mcp:outlook/search_mail")).toBeChecked();
    await expect(check("mcp:outlook/move_mail")).not.toBeChecked();
    await expect(check("mcp:outlook/send_mail")).not.toBeChecked();
    await expect(check("mcp:outlook/export_box")).not.toBeChecked();

    // WHEN the grant is confirmed and the sheet saved
    await dialog.getByTestId("agent-adjust-connection-confirm").click();
    await page.getByTestId("agent-adjust-save").click();

    // THEN the access holds the two reading tools, and each carries the remote badge on the sheet
    await expect(page.getByTestId("agent-adjust-save")).toHaveCount(0);
    expect(await writes(page)).toEqual([
      {
        cmd: "set_agent_override",
        args: {
          name: "apollia:chat",
          field: "access",
          value: ["file_read", "mcp:outlook/read_mail", "mcp:outlook/search_mail"],
        },
      },
    ]);
    await expect(page.getByTestId("agent-sheet-access-remote")).toHaveCount(2);
  });

  test("cancel drops the changes of the session and writes nothing", async ({ page }) => {
    // GIVEN the sheet of the Apollia agent in adjustment, its mission changed
    await openApollia(page);
    await page.getByTestId("agent-adjust-open").click();
    await page.getByTestId("agent-adjust-mission").fill("Something else");

    // WHEN the adjustment is cancelled
    await page.getByTestId("agent-adjust-cancel").click();

    // THEN the sheet reads again as before and nothing was written
    await expect(page.getByTestId("agent-sheet-mission-empty")).toBeVisible();
    expect(await writes(page)).toEqual([]);
  });
});
