/**
 * Granting a folder just added to the agent of the chat.
 *
 * A folder added as a connection is granted to no agent: an agent reads it
 * only once the tools of the folder are part of its accesses. The dialog that
 * follows the picker offers to give the reading tools to Apollia at once, so
 * that "I added my folder" and "Apollia reads it" are one gesture the person
 * sees and can decline.
 *
 * The runtime writes the grant: an Apollia on the default access keeps it,
 * read again at each run, and holds the folder on top of it.
 */
import { getAgentDefinition } from "$lib/ipc/agentSheet";
import { grantConnection } from "$lib/ipc/connections";
import { APOLLIA_AGENT_ID } from "$lib/chat/chatTargets";
import type { Connection } from "$lib/types";

/** The tools of `connection` that only read, and that the runtime offers checked. */
export function readingTools(connection: Connection): string[] {
  return connection.tools
    .filter((tool) => tool.prechecked && (tool.family === "read" || tool.family === "exempt"))
    .map((tool) => tool.id);
}

/**
 * `access` with the reading tools of `connection` added, in order and without
 * a duplicate. The same list comes back when nothing is to add.
 */
export function accessWithFolder(access: readonly string[], connection: Connection): string[] {
  const held = new Set(access);
  const added = readingTools(connection).filter((id) => !held.has(id));
  return added.length === 0 ? [...access] : [...access, ...added];
}

/**
 * Gives Apollia the reading tools of `connection`. Returns whether its
 * accesses changed; nothing is written when it already held them.
 */
export async function grantFolderToApollia(connection: Connection): Promise<boolean> {
  const agent = await getAgentDefinition(APOLLIA_AGENT_ID);
  const current = agent?.access ?? [];
  const next = accessWithFolder(current, connection);
  if (next.length === current.length) return false;
  await grantConnection(connection.id, [APOLLIA_AGENT_ID], next.slice(current.length));
  return true;
}
