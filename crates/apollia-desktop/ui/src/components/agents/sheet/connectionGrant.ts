/**
 * The decisions behind granting a connection to an agent from its sheet,
 * kept apart from the markup so they are tested on their own (Vitest runs in
 * `node`).
 *
 * A connection is granted tool by tool: its tools are shown in the five
 * action families, plus "unclassified" when one has no family, and the boxes
 * start as the runtime offers them (`prechecked`): only the reading tools of
 * a connection from the selection, or an official one, are checked. A
 * connection that sends data off this machine is marked remote, in the
 * dialog and on every access of the sheet that comes from it.
 */
import type { ActionFamily } from "$lib/tools/action-family";
import type { Connection, ConnectionProvenance } from "$lib/types";
import { readableName, type AccessGroupKey } from "./agentSheet";

/** A group of the connection dialog: an action family, or unclassified. */
export type ConnectionFamilyKey = Exclude<AccessGroupKey, "agents">;

/** The groups of the dialog, in order; `unclassified` only when it holds a tool. */
export const CONNECTION_FAMILY_ORDER: readonly ConnectionFamilyKey[] = [
  "read",
  "read_personal",
  "write_or_delete",
  "send_external",
  "execute_code",
  "unclassified",
];

/** One tool of the connection, as the dialog lists it. */
export interface ConnectionToolRow {
  /** The access identifier the agent is granted. */
  id: string;
  /** A readable name derived from the identifier. */
  name: string;
  /** What the tool does, as the tool registry describes it; empty when unknown. */
  description: string;
  family: ConnectionFamilyKey;
  /** Offered checked when the connection is granted. */
  prechecked: boolean;
  /** Already granted in the draft. */
  granted: boolean;
}

/** One family of the dialog. */
export interface ConnectionToolGroup {
  key: ConnectionFamilyKey;
  tools: ConnectionToolRow[];
}

/** What the connection dialog reads when it opens. */
export interface ConnectionOffer {
  connections: Connection[];
  /** Description of each tool, by identifier. */
  descriptions: Map<string, string>;
}

/** The group a tool of a connection is listed in. */
export function connectionFamily(family: ActionFamily | string): ConnectionFamilyKey {
  switch (family) {
    case "read":
    case "exempt":
      return "read";
    case "read_personal":
    case "write_or_delete":
    case "send_external":
    case "execute_code":
      return family;
    default:
      return "unclassified";
  }
}

/** `mcp:notion/search_pages` reads `Search pages`. */
export function connectionToolName(id: string): string {
  return readableName(id.slice(id.lastIndexOf("/") + 1));
}

/**
 * The tools of `connection` grouped by family. The five families are always
 * listed, an empty one says so; the unclassified group only when it holds a
 * tool. `descriptions` maps a tool identifier to its description.
 */
export function connectionToolGroups(
  connection: Connection,
  descriptions: ReadonlyMap<string, string>,
  granted: readonly string[],
): ConnectionToolGroup[] {
  const has = new Set(granted);
  const rows: ConnectionToolRow[] = [...connection.tools]
    .sort((a, b) => a.id.localeCompare(b.id))
    .map((tool) => ({
      id: tool.id,
      name: connectionToolName(tool.id),
      description: descriptions.get(tool.id) ?? "",
      family: connectionFamily(tool.family),
      prechecked: tool.prechecked,
      granted: has.has(tool.id),
    }));
  return CONNECTION_FAMILY_ORDER.map((key) => ({
    key,
    tools: rows.filter((row) => row.family === key),
  })).filter((group) => group.key !== "unclassified" || group.tools.length > 0);
}

/**
 * The tools checked when the dialog opens on `connection`: those the runtime
 * offers checked, less those the draft already grants.
 */
export function initialPicks(connection: Connection, granted: readonly string[]): string[] {
  const has = new Set(granted);
  return connection.tools.filter((t) => t.prechecked && !has.has(t.id)).map((t) => t.id);
}

/** Whether `accessId` is the connection itself or one of its tools. */
function belongsTo(accessId: string, connection: Connection): boolean {
  return (
    accessId === connection.id ||
    accessId.startsWith(`${connection.id}/`) ||
    connection.tools.some((t) => t.id === accessId)
  );
}

/** The connection `accessId` comes from, `null` when it comes from none. */
export function connectionOf(accessId: string, connections: readonly Connection[]): Connection | null {
  return connections.find((c) => belongsTo(accessId, c)) ?? null;
}

/** The accesses among `accessIds` that come from a remote connection. */
export function remoteAccesses(
  accessIds: readonly string[],
  connections: readonly Connection[],
): Set<string> {
  const remote = connections.filter((c) => c.remote);
  return new Set(accessIds.filter((id) => remote.some((c) => belongsTo(id, c))));
}

/** The accesses among `accessIds` that come from a connection its user paused. */
export function pausedAccesses(
  accessIds: readonly string[],
  connections: readonly Connection[],
): Set<string> {
  const paused = connections.filter((c) => c.status === "paused");
  return new Set(accessIds.filter((id) => paused.some((c) => belongsTo(id, c))));
}

/** The family of an access that is a tool of a connection, `null` otherwise. */
export function connectionToolFamily(
  accessId: string,
  connections: readonly Connection[],
): ActionFamily | null {
  for (const connection of connections) {
    const tool = connection.tools.find((t) => t.id === accessId);
    if (tool) return tool.family;
  }
  return null;
}

/** The name a connection is shown under: its label, then its account. */
export function connectionTitle(connection: Connection): string {
  return connection.account ? `${connection.label} · ${connection.account}` : connection.label;
}

// ── Catalogue keys ──────────────────────────────────────────────────────

/** Where a connection was found, as the dialog says it. */
export const CONNECTION_PROVENANCE_LABEL: Record<ConnectionProvenance, string> = {
  selection: "agent_adjust.connection_from_selection",
  registry: "agent_adjust.connection_from_registry",
  custom: "agent_adjust.connection_from_custom",
  package: "agent_adjust.connection_from_package",
};
