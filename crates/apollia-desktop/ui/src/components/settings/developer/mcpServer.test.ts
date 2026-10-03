import { describe, test, expect, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => null) }));

import {
  MCP_SERVER_KEY,
  exposedTestId,
  mcpClientConfig,
  mcpCommandPath,
  mcpSectionVisible,
  readMcpSwitch,
  setExposure,
  writeMcpSwitch,
  type McpServerIpc,
} from "./mcpServer";
import type { ResolvedSetting, SettingSource } from "$lib/ipc/settings";
import type { McpExposableItem } from "$lib/ipc/mcpServer";
import type { CliStatus } from "$lib/types";

const AGENT: McpExposableItem = {
  kind: "agent",
  item_id: "ledger-clerk",
  name: "Invoice sorter",
  description: "Sorts the incoming invoices.",
  exposed: false,
};
const WORKFLOW: McpExposableItem = {
  kind: "workflow",
  item_id: "acme/chase",
  name: "acme/chase",
  description: "Chases the unpaid invoices.",
  exposed: true,
};

/**
 * The shell and the runtime, held in memory: `file` is what `apollia.toml`
 * carries for the switch (`undefined` when absent), `exposed` the stored flags.
 */
function fakeShell(opts: { file?: boolean; source?: SettingSource; refuse?: boolean } = {}) {
  const state = {
    file: opts.file,
    exposed: new Map<string, boolean>([
      ["agent:ledger-clerk", false],
      ["workflow:acme/chase", true],
    ]),
  };
  const resolve = (): ResolvedSetting =>
    state.file === undefined
      ? { key: MCP_SERVER_KEY, value: false, source: "default" }
      : { key: MCP_SERVER_KEY, value: state.file, source: opts.source ?? "user" };
  const ipc: McpServerIpc = {
    readSetting: vi.fn(async () => resolve()),
    writeSetting: vi.fn(async (_key: string, value: unknown) => {
      if (opts.refuse) throw new Error("setting is locked by the organization policy");
      state.file = value === true;
      return resolve();
    }),
    listItems: vi.fn(async () =>
      [AGENT, WORKFLOW].map((item) => ({
        ...item,
        exposed: state.exposed.get(`${item.kind}:${item.item_id}`) === true,
      })),
    ),
    setExposed: vi.fn(async (kind, itemId, exposed) => {
      if (opts.refuse) throw new Error("no agent named 'ledger-clerk'");
      state.exposed.set(`${kind}:${itemId}`, exposed);
    }),
  };
  return { ipc, state };
}

describe("the MCP server section, in Settings > Developer", () => {
  test("it is shown with the developer mode on, and only then", () => {
    // GIVEN the two states of the developer mode
    // WHEN the section decides whether it is shown
    // THEN it is shown only with the mode on
    expect(mcpSectionVisible(true)).toBe(true);
    expect(mcpSectionVisible(false)).toBe(false);
  });
});

describe("the switch, read from and written to mcp.server_enabled", () => {
  test("without the key, the switch is off", async () => {
    // GIVEN an apollia.toml with no [mcp] server_enabled
    const { ipc } = fakeShell();

    // WHEN the switch is read
    const read = await readMcpSwitch(ipc);

    // THEN it is off, unlocked, read under the named key
    expect(read).toEqual({ enabled: false, locked: false });
    expect(ipc.readSetting).toHaveBeenCalledWith("mcp.server_enabled");
  });

  test("turning it on writes the setting and reads back on", async () => {
    // GIVEN the switch off
    const { ipc, state } = fakeShell();

    // WHEN it is turned on
    const written = await writeMcpSwitch(ipc, true);

    // THEN the file holds it, and the switch reads on
    expect(ipc.writeSetting).toHaveBeenCalledWith("mcp.server_enabled", true);
    expect(state.file).toBe(true);
    expect(written.enabled).toBe(true);
    expect((await readMcpSwitch(ipc)).enabled).toBe(true);
  });

  test("a value the policy imposes locks the switch", async () => {
    // GIVEN the policy imposing the bridge on
    const { ipc } = fakeShell({ file: true, source: "policy" });

    // WHEN the switch is read
    const read = await readMcpSwitch(ipc);

    // THEN it is on and locked
    expect(read).toEqual({ enabled: true, locked: true });
  });

  test("a refused write rejects, so the switch can come back", async () => {
    // GIVEN a shell that refuses every write
    const { ipc, state } = fakeShell({ refuse: true });

    // WHEN the switch is turned on
    const attempt = writeMcpSwitch(ipc, true);

    // THEN the write rejects and the file is untouched
    await expect(attempt).rejects.toThrow();
    expect(state.file).toBeUndefined();
  });

  test("an unreachable shell reads as off", async () => {
    // GIVEN a shell that cannot be reached
    const { ipc } = fakeShell();
    ipc.readSetting = vi.fn(async () => {
      throw new Error("ipc down");
    });

    // WHEN the switch is read
    const read = await readMcpSwitch(ipc);

    // THEN it is off
    expect(read.enabled).toBe(false);
  });
});

describe("the exposed checkbox of each agent and workflow", () => {
  test("checking an item stores its flag through the route and marks only it", async () => {
    // GIVEN an agent not exposed and a workflow exposed
    const { ipc, state } = fakeShell();
    const items = await ipc.listItems();

    // WHEN the agent is checked
    const next = await setExposure(ipc, items, items[0], true);

    // THEN the flag is stored under the agent's kind and id, and only it moved
    expect(ipc.setExposed).toHaveBeenCalledWith("agent", "ledger-clerk", true);
    expect(state.exposed.get("agent:ledger-clerk")).toBe(true);
    expect(next.map((i) => i.exposed)).toEqual([true, true]);
    expect(items.map((i) => i.exposed)).toEqual([false, true]);
  });

  test("unchecking a workflow withdraws it, and the next read agrees", async () => {
    // GIVEN the workflow exposed
    const { ipc } = fakeShell();
    const items = await ipc.listItems();

    // WHEN it is unchecked
    const next = await setExposure(ipc, items, items[1], false);

    // THEN the list and a fresh read both show it withdrawn
    expect(next[1].exposed).toBe(false);
    expect((await ipc.listItems())[1].exposed).toBe(false);
  });

  test("a refused flag rejects and leaves the list as it was", async () => {
    // GIVEN a runtime that refuses the write
    const { ipc } = fakeShell({ refuse: true });
    const items = [AGENT, WORKFLOW];

    // WHEN the agent is checked
    const attempt = setExposure(ipc, items, AGENT, true);

    // THEN it rejects, and the list is unchanged
    await expect(attempt).rejects.toThrow();
    expect(items[0].exposed).toBe(false);
  });

  test("each checkbox carries a test id built from its kind and identifier", () => {
    // GIVEN an agent and a workflow whose id has a slash
    // WHEN their test ids are built
    // THEN they are distinct and keep to a safe charset
    expect(exposedTestId(AGENT)).toBe("mcp-exposed-agent-ledger-clerk");
    expect(exposedTestId(WORKFLOW)).toBe("mcp-exposed-workflow-acme_chase");
  });
});

describe("the configuration to paste into an MCP client", () => {
  const status = (bundled_path: string | null): CliStatus => ({
    bundled: bundled_path !== null,
    bundled_path,
    installed: false,
    symlink_path: "/usr/local/bin/apollia-os",
    version: "0.3.0",
    needs_privilege: false,
  });

  test("the command is the CLI bundled in the application, by absolute path", () => {
    // GIVEN the CLI bundled in the application
    const path = "/Applications/Apollia.app/Contents/Resources/apollia-os";

    // WHEN the command path is chosen
    // THEN it is the bundled binary
    expect(mcpCommandPath(status(path))).toBe(path);
  });

  test("without a bundled CLI, the command is the link on the PATH, still absolute", () => {
    // GIVEN a development build with no bundled CLI
    // WHEN the command path is chosen
    const path = mcpCommandPath(status(null));

    // THEN it is the absolute link, and nothing is chosen before the status is read
    expect(path).toBe("/usr/local/bin/apollia-os");
    expect(path?.startsWith("/")).toBe(true);
    expect(mcpCommandPath(null)).toBeNull();
  });

  test("the block names the server, its absolute command and the bridge subcommand", () => {
    // GIVEN the bundled command
    const path = "/Applications/Apollia.app/Contents/Resources/apollia-os";

    // WHEN the configuration is written
    const config = JSON.parse(mcpClientConfig(path));

    // THEN a client reads one server, run as `<path> mcp server`
    expect(config).toEqual({
      mcpServers: { apollia: { command: path, args: ["mcp", "server"] } },
    });
  });
});
