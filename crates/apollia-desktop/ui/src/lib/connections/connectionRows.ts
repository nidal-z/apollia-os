/**
 * Rows of the single connection list.
 *
 * The list on the Connections screen is the answer of the loader
 * (`loadConnections`), whatever the kind: native accounts, mailboxes, servers
 * and folders sit in one list. Pure: the screen hands it the connections and
 * renders the rows; a test hands it plain objects.
 *
 * The accounts of one native provider fold into one row, because their page
 * (accounts, client, capabilities) is the provider's.
 */
import type { ProviderId } from "$lib/connections/capabilities";
import type { ConnectionStatus } from "$lib/connections/status";
import type {
  Connection,
  ConnectionKind,
  ConnectionProvenance,
  ConnectionState,
} from "$lib/types";

/** What the detail pane opens when a row is chosen. */
export type RowSelection =
  | { kind: "native"; provider: ProviderId }
  | { kind: "mcp"; name: string }
  | { kind: "connection"; id: string }
  /** The tools Apollia ships, a source of their own after the connections. */
  | { kind: "builtin" };

/** One row of the connection list. */
export interface ConnectionRow {
  /** Stable key, unique in the list. */
  key: string;
  kind: ConnectionKind;
  label: string;
  /** Accounts behind the row: several for a native provider, one or none otherwise. */
  accounts: string[];
  provenance: ConnectionProvenance;
  /** Sends data off this machine; true when one of its connections does. */
  remote: boolean;
  official: boolean;
  /** The least usable state among the connections of the row. */
  state: ConnectionState;
  status: ConnectionStatus;
  toolCount: number;
  selection: RowSelection;
}

/** The native providers Apollia ships, offered by the add sheet. */
export const NATIVE_PROVIDERS: readonly ProviderId[] = ["google", "microsoft"];

/** The catalogue keys that name and describe a native provider's page. */
export const NATIVE_PROVIDER_KEYS: Record<ProviderId, { name: string; description: string }> = {
  google: {
    name: "connections.native_google_name",
    description: "connections.native_google_description",
  },
  microsoft: {
    name: "connections.native_microsoft_name",
    description: "connections.native_microsoft_description",
  },
};

/** Order of the states from the least to the most usable. */
const STATE_RANK: Record<ConnectionState, number> = {
  forbidden_by_policy: -1,
  unavailable: 0,
  needs_reauth: 1,
  degraded: 2,
  stopped: 3,
  connected: 4,
};

/** The native provider of a connection, read from its identifier `native:<provider>:<account>`. */
export function nativeProviderOf(connection: Connection): ProviderId | null {
  if (connection.kind !== "native") return null;
  const provider = connection.id.split(":")[1];
  return NATIVE_PROVIDERS.find((p) => p === provider) ?? null;
}

/** The coarse status that drives the dot and the filter chips. */
export function statusOfState(state: ConnectionState): ConnectionStatus {
  switch (state) {
    case "connected":
      return "active";
    case "degraded":
    case "needs_reauth":
      return "attention";
    case "unavailable":
    case "forbidden_by_policy":
      return "error";
    case "stopped":
      return "idle";
  }
}

function worst(a: ConnectionState, b: ConnectionState): ConnectionState {
  return STATE_RANK[a] <= STATE_RANK[b] ? a : b;
}

function serverName(connection: Connection): string {
  return connection.id.startsWith("mcp:") ? connection.id.slice(4) : connection.label;
}

function single(connection: Connection, label: string): ConnectionRow {
  const selection: RowSelection =
    connection.kind === "mcp"
      ? { kind: "mcp", name: serverName(connection) }
      : { kind: "connection", id: connection.id };
  return {
    key: connection.id,
    kind: connection.kind,
    label,
    accounts: connection.account ? [connection.account] : [],
    provenance: connection.provenance,
    remote: connection.remote,
    official: connection.official,
    state: connection.state,
    status: statusOfState(connection.state),
    toolCount: connection.tools.length,
    selection,
  };
}

/**
 * The rows of the list, in the order of the loader, one per connection except
 * the accounts of a native provider, folded into one row at the place of the
 * first. `labelFor` names a row (a server's display name); it defaults to the
 * connection's own label.
 */
export function connectionRows(
  connections: readonly Connection[],
  labelFor: (connection: Connection) => string = (c) => c.label,
): ConnectionRow[] {
  const rows: ConnectionRow[] = [];
  const nativeRows = new Map<ProviderId, ConnectionRow>();
  for (const connection of connections) {
    const provider = nativeProviderOf(connection);
    if (provider === null) {
      rows.push(single(connection, labelFor(connection)));
      continue;
    }
    const existing = nativeRows.get(provider);
    if (existing) {
      if (connection.account) existing.accounts.push(connection.account);
      existing.remote ||= connection.remote;
      existing.state = worst(existing.state, connection.state);
      existing.status = statusOfState(existing.state);
      existing.toolCount += connection.tools.length;
      continue;
    }
    const row: ConnectionRow = {
      ...single(connection, labelFor(connection)),
      key: `native:${provider}`,
      selection: { kind: "native", provider },
    };
    nativeRows.set(provider, row);
    rows.push(row);
  }
  return rows;
}

/** Whether a row answers the text typed in the filter (label or account). */
export function rowMatches(row: ConnectionRow, query: string): boolean {
  const q = query.trim().toLowerCase();
  if (q === "") return true;
  return (
    row.label.toLowerCase().includes(q) || row.accounts.some((a) => a.toLowerCase().includes(q))
  );
}

/** Whether a row is the one the detail pane shows. */
export function isSelected(row: ConnectionRow, selection: RowSelection | null): boolean {
  if (selection === null || selection.kind !== row.selection.kind) return false;
  switch (selection.kind) {
    case "native":
      return row.selection.kind === "native" && row.selection.provider === selection.provider;
    case "mcp":
      return row.selection.kind === "mcp" && row.selection.name === selection.name;
    case "connection":
      return row.selection.kind === "connection" && row.selection.id === selection.id;
    case "builtin":
      return false;
  }
}

/**
 * The provenance line shown under a row in developer mode only: the technical
 * kind and where it was found (plain identifiers: the developer's reading of
 * the row), and the translated "official" mark when it is official.
 */
export function provenanceLine(row: ConnectionRow, officialLabel: string): string {
  const kind = row.kind === "mcp" ? "MCP" : row.kind;
  const parts = [kind, row.provenance];
  if (row.official) parts.push(officialLabel);
  return parts.join(" · ");
}
