/**
 * Loader for the single connection list.
 *
 * Pure apart from the injected fetcher: the screen hands it nothing and gets
 * `listConnections`, a test hands it a stub. Errors are returned as a state,
 * never thrown, so the caller renders one of three states and nothing else.
 */
import { listConnections } from "$lib/ipc/connections";
import type { Connection, ConnectionKind } from "$lib/types";

/** What the connection list shows. */
export type ConnectionListState =
  | { status: "loading" }
  | { status: "ready"; connections: Connection[] }
  | { status: "error"; message: string };

/** Order of the kinds in the list: accounts, mailboxes, servers, folders. */
const KIND_ORDER: Record<ConnectionKind, number> = { native: 0, imap: 1, mcp: 2, folder: 3 };

/** Sort connections by kind, then label, then account. */
export function sortConnections(connections: readonly Connection[]): Connection[] {
  return [...connections].sort(
    (a, b) =>
      KIND_ORDER[a.kind] - KIND_ORDER[b.kind] ||
      a.label.localeCompare(b.label) ||
      (a.account ?? "").localeCompare(b.account ?? ""),
  );
}

/** Identifiers of the tools offered checked when `connection` is granted. */
export function precheckedToolIds(connection: Connection): string[] {
  return connection.tools.filter((t) => t.prechecked).map((t) => t.id);
}

/** The connections the organisation policy forbade at the last start. */
export function forbiddenConnections(connections: readonly Connection[]): Connection[] {
  return sortConnections(connections.filter((c) => c.state === "forbidden_by_policy"));
}

/** Load the list once. */
export async function loadConnections(
  fetcher: () => Promise<Connection[]> = listConnections,
): Promise<ConnectionListState> {
  try {
    return { status: "ready", connections: sortConnections(await fetcher()) };
  } catch (e) {
    return { status: "error", message: e instanceof Error ? e.message : String(e) };
  }
}
