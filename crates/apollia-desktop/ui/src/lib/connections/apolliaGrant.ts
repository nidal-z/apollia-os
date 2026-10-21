/**
 * Granting a connection just added to the agent of the chat.
 *
 * A connection added from the selection or by its address is granted to no
 * agent: the chat could not use it until its tools were checked on the sheet
 * of Apollia. The add dialogs offer, checked, to give Apollia every tool of
 * the connection at once. What a tool may do is still decided at each call:
 * a sensitive one, and one nobody classified, asks for approval first.
 *
 * The runtime writes the grant: an Apollia on the default access keeps it,
 * read again at each run, and holds the tools of the connection on top of it.
 */
import { getAgentDefinition } from "$lib/ipc/agentSheet";
import { grantConnection, listConnections } from "$lib/ipc/connections";
import { APOLLIA_AGENT_ID } from "$lib/chat/chatTargets";
import type { Connection } from "$lib/types";

/** The identifier of the connection of the MCP server `name`. */
export function mcpConnectionId(name: string): string {
  return `mcp:${name}`;
}

/** Every tool of the connection `id` in `connections`, none when it is not listed. */
export function connectionTools(connections: readonly Connection[], id: string): string[] {
  return connections.find((c) => c.id === id)?.tools.map((tool) => tool.id) ?? [];
}

/**
 * `access` with `tools` added, in order and without a duplicate. The same
 * list comes back when nothing is to add.
 */
export function accessWithTools(access: readonly string[], tools: readonly string[]): string[] {
  const held = new Set(access);
  const added = tools.filter((id, i) => !held.has(id) && tools.indexOf(id) === i);
  return added.length === 0 ? [...access] : [...access, ...added];
}

export interface GrantOptions {
  /** How the connections are read; the Tauri command by default. */
  list?: () => Promise<Connection[]>;
  /** How many times the connection is read while its tools are not yet indexed. */
  attempts?: number;
  /** Wait between two reads, in milliseconds. */
  waitMs?: number;
}

/**
 * Gives Apollia every tool of the MCP server `name`. A server just started
 * may not have indexed its tools yet, so the list is read again a few times
 * while the connection offers none. Returns how many tools were added.
 */
export async function grantConnectionToApollia(
  name: string,
  options: GrantOptions = {},
): Promise<number> {
  const list = options.list ?? listConnections;
  const attempts = Math.max(1, options.attempts ?? 6);
  const waitMs = options.waitMs ?? 500;
  const id = mcpConnectionId(name);
  let tools: string[] = [];
  for (let i = 0; i < attempts; i += 1) {
    tools = connectionTools(await list(), id);
    if (tools.length > 0) break;
    if (i + 1 < attempts) await new Promise((resolve) => setTimeout(resolve, waitMs));
  }
  if (tools.length === 0) return 0;
  const agent = await getAgentDefinition(APOLLIA_AGENT_ID);
  const current = agent?.access ?? [];
  const next = accessWithTools(current, tools);
  if (next.length === current.length) return 0;
  await grantConnection(id, [APOLLIA_AGENT_ID], next.slice(current.length));
  return next.length - current.length;
}
