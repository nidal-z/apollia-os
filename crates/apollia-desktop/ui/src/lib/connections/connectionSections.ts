/**
 * The decisions behind the three sections of a connection card that look past
 * the connection itself: the agents that use it, what they did with it this
 * week, and its tools with their family. Kept apart from the markup so they are
 * tested on their own.
 */
import { APOLLIA_AGENT_ID } from "$lib/chat/chatTargets";
import type { I18nDescriptor } from "$lib/connections/status";
import type { ConnectionAgent } from "$lib/ipc/connections";
import type { ActionFamily } from "$lib/tools/action-family";
import type { AgentListItem, Connection, ConnectionTool } from "$lib/types";

/** The period the activity section reads, in days. */
export const ACTIVITY_DAYS = 7;

/** The families an operator may give a tool, in the order the screens list them. */
export const RECLASSIFY_FAMILIES: readonly ActionFamily[] = [
  "read",
  "read_personal",
  "write_or_delete",
  "send_external",
  "execute_code",
];

const FAMILY_LABEL: Record<string, string> = {
  read: "agent_sheet.family_read",
  read_personal: "agent_sheet.family_read_personal",
  write_or_delete: "agent_sheet.family_write_or_delete",
  send_external: "agent_sheet.family_send_external",
  execute_code: "agent_sheet.family_execute_code",
};

/** The label of a tool's family; a family the screens do not name reads unclassified. */
export function familyLabel(family: ActionFamily | string): string {
  return FAMILY_LABEL[family] ?? "agent_sheet.family_unclassified";
}

/**
 * The prefix every tool of `connection` is named under, as the journal stores
 * it: `mcp:notion/`, `conn:google:ada@example.com/`, `conn:folder:<id>/`.
 * `null` for a mailbox, whose tools are common to every mailbox and carry no
 * prefix of their own.
 */
export function toolPrefixOf(connection: Pick<Connection, "id" | "kind">): string | null {
  switch (connection.kind) {
    case "mcp":
      return `${connection.id}/`;
    case "native":
      return connection.id.startsWith("native:") ? `conn:${connection.id.slice("native:".length)}/` : null;
    case "folder":
      return connection.id.startsWith("folder:") && connection.id.length > "folder:".length
        ? `conn:folder:${connection.id.slice("folder:".length)}/`
        : null;
    default:
      return null;
  }
}

/** The local name of a tool of a service: `mcp:acme/create_page` reads `create_page`. */
export function localToolName(toolId: string): string {
  return toolId.slice(toolId.lastIndexOf("/") + 1);
}

/**
 * Whether the operator may give `tool` of `connection` another family: a tool
 * of a service added outside the selection, that the selection does not
 * classify. The runtime holds the same rule and refuses anything else.
 */
export function canReclassify(connection: Pick<Connection, "kind" | "provenance">, tool: ConnectionTool): boolean {
  return connection.kind === "mcp" && connection.provenance !== "selection" && tool.familySource !== "selection";
}

/** The tool `localName` of `connection`, `null` when it lists none by that name. */
export function toolOf(connection: Pick<Connection, "tools">, localName: string): ConnectionTool | null {
  return connection.tools.find((tool) => localToolName(tool.id) === localName) ?? null;
}

/** One agent the user may give the connection to. */
export interface AgentChoice {
  /** The identifier the grant names. */
  id: string;
  name: string;
}

/**
 * The agents offered by "Give to an agent": Apollia first, then every
 * installed agent by name, less those that hold a tool of the connection
 * already. `apollia` is how Apollia is named on the screen.
 */
export function agentsToOffer(
  installed: readonly AgentListItem[],
  holders: readonly ConnectionAgent[],
  apollia: string,
): AgentChoice[] {
  const held = new Set(holders.map((agent) => agent.id));
  const others = installed
    .filter((agent) => agent.name !== APOLLIA_AGENT_ID)
    .map((agent) => ({ id: agent.name, name: agent.name }))
    .sort((a, b) => a.name.localeCompare(b.name));
  return [{ id: APOLLIA_AGENT_ID, name: apollia }, ...others].filter((agent) => !held.has(agent.id));
}

/** The tools a grant from the card gives: every tool of the connection. */
export function grantedTools(connection: Pick<Connection, "tools">): string[] {
  return connection.tools.map((tool) => tool.id);
}

/** The start of the activity period, `days` before `now`, as an RFC 3339 instant. */
export function activitySince(now: number, days: number = ACTIVITY_DAYS): string {
  return new Date(now - days * 86_400_000).toISOString();
}

/** How long ago `at` was, at `now`; null when absent, unreadable or ahead of `now`. */
export function usedDescriptor(at: string | null, now: number): I18nDescriptor | null {
  if (!at) return null;
  const diffMs = now - new Date(at).getTime();
  if (Number.isNaN(diffMs) || diffMs < 0) return null;
  const mins = Math.floor(diffMs / 60_000);
  if (mins < 1) return { key: "connections.card.activity.used_just_now" };
  if (mins < 60) return { key: "connections.card.activity.used_minutes_ago", values: { mins } };
  const hours = Math.floor(mins / 60);
  if (hours < 24) return { key: "connections.card.activity.used_hours_ago", values: { hours } };
  return { key: "connections.card.activity.used_days_ago", values: { days: Math.floor(hours / 24) } };
}
