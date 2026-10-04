/**
 * Typed Tauri command wrappers for Apollia as an MCP server: the agents and
 * workflows an MCP client may call. The switch itself is the named setting
 * `mcp.server_enabled`, read and written through `$lib/ipc/settings`.
 */
import { invoke } from "@tauri-apps/api/core";

/** What an exposable item is. */
export type McpExposableKind = "agent" | "workflow";

/** One agent or workflow an MCP client may be given, with its flag. */
export interface McpExposableItem {
  kind: McpExposableKind;
  /** The agent id, or `<package>/<workflow>`. */
  item_id: string;
  name: string;
  description: string;
  exposed: boolean;
}

/** Every agent that takes a message and every workflow, each with its flag. */
export function listMcpExposable(): Promise<McpExposableItem[]> {
  return invoke<McpExposableItem[]>("list_mcp_exposable");
}

/** Marks one agent or workflow exposed to MCP clients, or not. */
export function setMcpExposed(
  kind: McpExposableKind,
  itemId: string,
  exposed: boolean,
): Promise<void> {
  return invoke<void>("set_mcp_exposed", { kind, itemId, exposed });
}
