/**
 * Rows of the single connection list.
 *
 * The list on the Connections screen is the answer of the loader
 * (`loadConnections`), whatever the kind: native accounts, mailboxes, services
 * and folders sit in one list, one row per connection, so each account of a
 * provider has its own row and its own state. Pure: the screen hands it the
 * connections and renders the rows; a test hands it plain objects.
 */
import type { ProviderId } from "$lib/connections/capabilities";
import type {
  Connection,
  ConnectionKind,
  ConnectionProvenance,
  ConnectionReason,
  ConnectionStatusKey,
} from "$lib/types";

/** What the detail card shows. */
export type RowSelection =
  /** One connection of the list, by its `Connection.id`. */
  | { kind: "connection"; id: string }
  /** The page of a native provider with no account yet, opened to connect one. */
  | { kind: "native"; provider: ProviderId }
  /** The tools Apollia ships, a source of their own after the connections. */
  | { kind: "builtin" };

/** One row of the connection list. */
export interface ConnectionRow {
  /** The `Connection.id`, unique in the list. */
  key: string;
  kind: ConnectionKind;
  label: string;
  account: string | null;
  provenance: ConnectionProvenance;
  /** Sends data off this machine. */
  remote: boolean;
  official: boolean;
  status: ConnectionStatusKey;
  reason: ConnectionReason | null;
  checkedAt: string | null;
  /** Name of the package that brought it, when one did. */
  package: string | null;
  toolCount: number;
}

/** The kind chips over the list: every kind, or one. */
export type KindFilter = "all" | ConnectionKind;

/** The word of each kind, on its chip and on the icon of a row. */
export const KIND_LABEL_KEYS: Record<KindFilter, string> = {
  all: "connections.list.kind_all",
  native: "connections.list.kind_native",
  imap: "connections.list.kind_imap",
  mcp: "connections.list.kind_mcp",
  folder: "connections.list.kind_folder",
};

/** The chips in their order. */
export const KIND_FILTERS: readonly KindFilter[] = ["all", "native", "imap", "mcp", "folder"];

/** The native providers Apollia ships, offered by the add window. */
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

/** The native provider of a connection, read from its identifier `native:<provider>:<account>`. */
export function nativeProviderOf(connection: Pick<Connection, "id" | "kind">): ProviderId | null {
  if (connection.kind !== "native") return null;
  const provider = connection.id.split(":")[1];
  return NATIVE_PROVIDERS.find((p) => p === provider) ?? null;
}

/** The server name of a service connection `mcp:<server>`, or null for another kind. */
export function serverNameOf(id: string): string | null {
  return id.startsWith("mcp:") ? id.slice(4) : null;
}

/**
 * One row per connection, what needs repair first, then the rest by name and
 * account. `labelFor` names a row (a service's display name); it defaults to
 * the connection's own label.
 */
export function connectionRows(
  connections: readonly Connection[],
  labelFor: (connection: Connection) => string = (c) => c.label,
): ConnectionRow[] {
  return sortRows(
    connections.map((c) => ({
      key: c.id,
      kind: c.kind,
      label: labelFor(c),
      account: c.account,
      provenance: c.provenance,
      remote: c.remote,
      official: c.official,
      status: c.status,
      reason: c.reason,
      checkedAt: c.checkedAt,
      package: c.package,
      toolCount: c.tools.length,
    })),
  );
}

/** What needs repair first, then by name, then by account. Returns a new array. */
export function sortRows(rows: readonly ConnectionRow[]): ConnectionRow[] {
  const repair = (r: ConnectionRow): number => (r.status === "needs_repair" ? 0 : 1);
  return [...rows].sort(
    (a, b) =>
      repair(a) - repair(b) ||
      a.label.localeCompare(b.label) ||
      (a.account ?? "").localeCompare(b.account ?? ""),
  );
}

/** Whether a row answers the text typed in the search (name, account or package). */
export function rowMatches(row: ConnectionRow, query: string): boolean {
  const q = query.trim().toLowerCase();
  if (q === "") return true;
  return [row.label, row.account, row.package].some((v) => v?.toLowerCase().includes(q) ?? false);
}

/** The rows the search and the kind chip let through, in their order. */
export function visibleRows(rows: readonly ConnectionRow[], query: string, kind: KindFilter): ConnectionRow[] {
  return rows.filter((r) => (kind === "all" || r.kind === kind) && rowMatches(r, query));
}

/** How many rows each chip holds. */
export function kindCounts(rows: readonly ConnectionRow[]): Record<KindFilter, number> {
  const counts: Record<KindFilter, number> = { all: rows.length, native: 0, imap: 0, mcp: 0, folder: 0 };
  for (const row of rows) counts[row.kind] += 1;
  return counts;
}

/** Whether a row is the one the detail card shows. */
export function isSelected(row: ConnectionRow, selection: RowSelection | null): boolean {
  return selection?.kind === "connection" && selection.id === row.key;
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
