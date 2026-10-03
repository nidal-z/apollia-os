// Apollia as an MCP server, in Settings > Developer: the switch, the agents
// and workflows an MCP client may call, and the configuration to paste in it.
//
// The decisions live here, apart from the component, so Vitest can hold them
// without rendering anything.

import type { ResolvedSetting } from "$lib/ipc/settings";
import type { McpExposableItem, McpExposableKind } from "$lib/ipc/mcpServer";
import type { CliStatus } from "$lib/types";

/** The dotted key of the switch in `apollia.toml`. */
export const MCP_SERVER_KEY = "mcp.server_enabled";

/** The subcommand an MCP client runs: the stdio bridge to the daemon. */
export const MCP_SERVER_ARGS = ["mcp", "server"] as const;

/** The name the configuration gives the server in the client. */
export const MCP_CLIENT_SERVER_NAME = "apollia";

/** The shell calls the section makes, replaceable in tests. */
export interface McpServerIpc {
  readSetting: (key: string) => Promise<ResolvedSetting>;
  writeSetting: (key: string, value: unknown) => Promise<ResolvedSetting>;
  listItems: () => Promise<McpExposableItem[]>;
  setExposed: (kind: McpExposableKind, itemId: string, exposed: boolean) => Promise<void>;
}

/** The switch as the section shows it. */
export interface McpSwitch {
  enabled: boolean;
  /** True when the organization policy imposes the value. */
  locked: boolean;
}

/** The section exists only with the developer mode on. */
export function mcpSectionVisible(developerMode: boolean): boolean {
  return developerMode;
}

function toSwitch(resolved: ResolvedSetting): McpSwitch {
  return { enabled: resolved.value === true, locked: resolved.source === "policy" };
}

/** Reads the switch; off when the shell cannot be reached. */
export async function readMcpSwitch(ipc: McpServerIpc): Promise<McpSwitch> {
  try {
    return toSwitch(await ipc.readSetting(MCP_SERVER_KEY));
  } catch {
    return { enabled: false, locked: false };
  }
}

/**
 * Writes the switch and returns it as the shell resolved it. Rejects when the
 * write is refused (imposed by the policy, unreadable file).
 */
export async function writeMcpSwitch(ipc: McpServerIpc, on: boolean): Promise<McpSwitch> {
  return toSwitch(await ipc.writeSetting(MCP_SERVER_KEY, on));
}

/** `items` with the flag of `kind`/`itemId` set to `exposed`. */
export function withExposure(
  items: readonly McpExposableItem[],
  kind: McpExposableKind,
  itemId: string,
  exposed: boolean,
): McpExposableItem[] {
  return items.map((item) =>
    item.kind === kind && item.item_id === itemId ? { ...item, exposed } : item,
  );
}

/**
 * Stores the flag of `item` and returns the list with it. Rejects, leaving
 * `items` untouched, when the runtime refuses.
 */
export async function setExposure(
  ipc: McpServerIpc,
  items: readonly McpExposableItem[],
  item: McpExposableItem,
  exposed: boolean,
): Promise<McpExposableItem[]> {
  await ipc.setExposed(item.kind, item.item_id, exposed);
  return withExposure(items, item.kind, item.item_id, exposed);
}

/** The test id of an item's checkbox, stable across names. */
export function exposedTestId(item: McpExposableItem): string {
  return `mcp-exposed-${item.kind}-${item.item_id.replace(/[^A-Za-z0-9_-]/g, "_")}`;
}

/**
 * The absolute path of the command an MCP client runs: the CLI bundled in the
 * application, else the link the application installs on the PATH.
 */
export function mcpCommandPath(status: CliStatus | null): string | null {
  if (!status) return null;
  return status.bundled_path ?? status.symlink_path;
}

/** The configuration to paste into an MCP client, for the command at `path`. */
export function mcpClientConfig(path: string): string {
  const config = {
    mcpServers: {
      [MCP_CLIENT_SERVER_NAME]: { command: path, args: [...MCP_SERVER_ARGS] },
    },
  };
  return JSON.stringify(config, null, 2);
}
