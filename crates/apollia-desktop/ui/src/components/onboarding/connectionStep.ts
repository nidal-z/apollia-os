/**
 * The decisions of the "useful connection" step of the first launch.
 *
 * The step proposes the connections the agent kept at the step before
 * requires and this machine lacks, and those only. It is optional twice over:
 * it is passed over when there is nothing to propose (no agent kept, an agent
 * created in the step, a package that requires nothing, every required
 * connection already there), and "Skip" is offered on it whatever it shows.
 *
 * What the package requires is read from its manifest as the installation
 * recorded it, so a package installed from a file and one taken from an
 * example are read the same way.
 *
 * Kept free of Svelte and of IPC so each decision is tested on its own; the
 * sources are handed in, `connectionStepLive.ts` hands the real ones.
 */
import { deriveServerName } from "$lib/integrations/connectorConfig";
import { connectionsPackage, proposesConnections, type FirstAgent } from "$lib/onboarding/firstAgent";
import { findSelectionEntry } from "$lib/stores/connectionAdd";
import type { Connection, RegistryServerView } from "$lib/types";

/** A connection a package requires, as its manifest names it. */
export interface RequiredConnection {
  /** Connection type: the name the selection lists it under. */
  type: string;
  /** Why the package needs it. */
  reason: string;
}

function namesOf(list: unknown, key: string): { name: string; entry: Record<string, unknown> }[] {
  const named: { name: string; entry: Record<string, unknown> }[] = [];
  for (const entry of Array.isArray(list) ? list : []) {
    if (typeof entry !== "object" || entry === null) continue;
    const name = (entry as Record<string, unknown>)[key];
    if (typeof name !== "string" || name.trim().length === 0) continue;
    named.push({ name: name.trim(), entry: entry as Record<string, unknown> });
  }
  return named;
}

/**
 * The connections the recorded `manifest` requires, each type once. One the
 * package brings itself is left out: installing the package added it. A
 * manifest this interface cannot read requires nothing, never an error.
 */
export function requiredConnections(manifest: unknown): RequiredConnection[] {
  if (typeof manifest !== "object" || manifest === null) return [];
  const { requires, mcp_servers: servers } = manifest as Record<string, unknown>;
  const brought = new Set(namesOf(servers, "name").map((server) => server.name.toLowerCase()));
  const seen = new Set<string>();
  const required: RequiredConnection[] = [];
  for (const { name, entry } of namesOf(requires, "type")) {
    const key = name.toLowerCase();
    if (brought.has(key) || seen.has(key)) continue;
    seen.add(key);
    required.push({ type: name, reason: typeof entry.reason === "string" ? entry.reason : "" });
  }
  return required;
}

/**
 * The type of a connection, as a package names it: `imap` for a mailbox,
 * `folder` for a folder, else the segment after the kind in its identifier
 * (`mcp:notion` is `notion`, `native:google:<account>` is `google`). The same
 * reading the preview of a package makes before it is installed.
 */
export function connectionType(connection: Pick<Connection, "id" | "kind">): string {
  if (connection.kind === "imap" || connection.kind === "folder") return connection.kind;
  return (connection.id.split(":")[1] ?? "").toLowerCase();
}

/** One line of the step: a required connection and whether it is there. */
export interface ConnectionLine {
  type: string;
  /** Name shown: the one the selection gives the entry, else the type. */
  label: string;
  reason: string;
  state: "missing" | "present";
  /** The selection entry "Add" opens the add dialog on; `null` when the selection holds none. */
  entry: RegistryServerView | null;
}

function present(
  type: string,
  entry: RegistryServerView | null,
  connections: readonly Pick<Connection, "id" | "kind">[],
): boolean {
  const types = new Set(connections.map(connectionType));
  if (types.has(type.toLowerCase())) return true;
  // A connection added from the selection is named after its entry.
  return entry !== null && types.has(deriveServerName(entry).toLowerCase());
}

/**
 * The lines the step opens with: each connection of `required` that
 * `connections` does not hold. A connection already there is not listed.
 */
export function missingLines(
  required: readonly RequiredConnection[],
  connections: readonly Pick<Connection, "id" | "kind">[],
  entries: readonly RegistryServerView[],
): ConnectionLine[] {
  const lines: ConnectionLine[] = [];
  for (const { type, reason } of required) {
    const entry = findSelectionEntry(entries, type);
    if (present(type, entry, connections)) continue;
    const label = entry?.enrichment?.operator_label ?? entry?.title ?? type;
    lines.push({ type, label: label.trim() || type, reason, state: "missing", entry });
  }
  return lines;
}

/**
 * The same lines once `connections` was read again: a connection that is now
 * there, or that the add dialog reported as added (`added`, by type), shows
 * as present. No line is dropped: the step keeps showing what it proposed.
 */
export function refreshedLines(
  lines: readonly ConnectionLine[],
  connections: readonly Pick<Connection, "id" | "kind">[],
  added: ReadonlySet<string> = new Set(),
): ConnectionLine[] {
  return lines.map((line) => ({
    ...line,
    state:
      added.has(line.type) || line.state === "present" || present(line.type, line.entry, connections)
        ? "present"
        : "missing",
  }));
}

/** What the step shows: the agent it is about and its lines. */
export interface ConnectionProposal {
  /** Name of the agent kept at the step before. */
  agentName: string;
  lines: ConnectionLine[];
}

/** Where the step reads from. */
export interface ConnectionSources {
  /** The manifest of the installed package `name`, as recorded. */
  manifest: (name: string) => Promise<unknown>;
  /** The connections of this machine. */
  connections: () => Promise<Connection[]>;
  /** The selection the add dialog opens on; read without network. */
  selection: () => Promise<RegistryServerView[]>;
}

/**
 * What the step proposes for `agent`. `null` without an agent, or for an
 * agent that came from no package. A package or a connection list that
 * cannot be read proposes nothing either: the step never claims a connection
 * is missing without having read that it is.
 */
export async function loadConnectionProposal(
  agent: FirstAgent | null,
  sources: ConnectionSources,
): Promise<ConnectionProposal | null> {
  const packageName = connectionsPackage(agent);
  if (agent === null || packageName === null || !proposesConnections(agent)) return null;
  try {
    const required = requiredConnections(await sources.manifest(packageName));
    if (required.length === 0) return { agentName: agent.name, lines: [] };
    const connections = await sources.connections();
    const entries = await sources.selection().catch(() => [] as RegistryServerView[]);
    return { agentName: agent.name, lines: missingLines(required, connections, entries) };
  } catch {
    return null;
  }
}

/** The step is passed over: there is no agent, or no connection of it is missing. */
export function nothingToPropose(proposal: ConnectionProposal | null): boolean {
  return proposal === null || proposal.lines.length === 0;
}

/** Whether every line of the step is there now. */
export function allPresent(lines: readonly ConnectionLine[]): boolean {
  return lines.every((line) => line.state === "present");
}

/** The sentence under the title, by what is still missing and how many lines there are. */
export function subtitleKey(lines: readonly ConnectionLine[]): string {
  const many = lines.length > 1;
  if (allPresent(lines)) {
    return many
      ? "onboarding.connection.subtitle_present_many"
      : "onboarding.connection.subtitle_present_one";
  }
  return many
    ? "onboarding.connection.subtitle_missing_many"
    : "onboarding.connection.subtitle_missing_one";
}

/**
 * The selection entry "Add" opens the add dialog on for the line `type`.
 * `null` for a line that is there already, or that the selection does not
 * hold: that one is added later, from the Connections screen.
 */
export function entryToAdd(lines: readonly ConnectionLine[], type: string): RegistryServerView | null {
  const line = lines.find((candidate) => candidate.type === type);
  return line === undefined || line.state === "present" ? null : line.entry;
}
