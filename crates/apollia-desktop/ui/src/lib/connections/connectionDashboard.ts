/**
 * The decisions of the Connections dashboard: the summary over the grid, what
 * each tile says beyond the connection itself (the agents that use it, its
 * last use, what it brings in words), and the suggestions of an empty page.
 *
 * The runtime has no grouped read of the agents and of the activity of every
 * connection: each is read connection by connection, through the two commands
 * the card already calls. `loadTileFacts` bounds that reading, so a machine
 * with many connections does not fire two commands per tile at once.
 *
 * Pure and i18n-agnostic: it returns keys and values, the components resolve
 * them with the live formatter; a test hands it plain objects.
 */
import { APOLLIA_AGENT_ID } from "$lib/chat/chatTargets";
import { activitySince, toolPrefixOf } from "$lib/connections/connectionSections";
import { brandLogoPath } from "$lib/connections/brandLogo";
import { needsAction, type ConnectionRow } from "$lib/connections/connectionRows";
import type { ProviderId } from "$lib/connections/capabilities";
import type { ConnectionActivity, ConnectionAgents } from "$lib/ipc/connections";
import type { Connection, ConnectorEnrichmentView } from "$lib/types";

/** The connections whose agents and activity are read, at most, for their tiles. */
export const MAX_TILE_READS = 24;

/** The connections read at the same time. */
export const TILE_READ_CONCURRENCY = 4;

/** What a tile says beyond the connection: who uses it, and when it was last used. */
export interface TileFacts {
  /** The agents that hold at least one of its tools, by identifier. */
  agents: string[];
  /** When one of its tools was last used over the period read; null when none was, or unknown. */
  lastUsedAt: string | null;
}

/** The two reads behind a tile, the commands of the card. */
export interface TileReaders {
  agents: (connectionId: string) => Promise<ConnectionAgents | null>;
  activity: (toolPrefix: string, since: string) => Promise<ConnectionActivity | null>;
}

/** The facts of one connection; a read that fails leaves its part empty. */
async function factsOf(
  connection: Pick<Connection, "id" | "kind">,
  readers: TileReaders,
  since: string,
): Promise<TileFacts> {
  const prefix = toolPrefixOf(connection);
  const [agents, activity] = await Promise.all([
    readers.agents(connection.id).catch(() => null),
    prefix === null ? Promise.resolve(null) : readers.activity(prefix, since).catch(() => null),
  ]);
  return {
    agents: (agents?.agents ?? []).map((agent) => agent.id),
    lastUsedAt: activity?.lastUsedAt ?? null,
  };
}

/**
 * The facts of the tiles of `connections`, in their order: the first
 * `limit` only, `concurrency` at a time, the activity over the period that
 * ends at `now`. A connection past the limit has no facts and its tile says
 * nothing of its agents.
 */
export async function loadTileFacts(
  connections: readonly Pick<Connection, "id" | "kind">[],
  readers: TileReaders,
  now: number,
  limit: number = MAX_TILE_READS,
  concurrency: number = TILE_READ_CONCURRENCY,
): Promise<Map<string, TileFacts>> {
  const queue = connections.slice(0, limit);
  const since = activitySince(now);
  const facts = new Map<string, TileFacts>();
  let next = 0;
  async function worker(): Promise<void> {
    while (next < queue.length) {
      const connection = queue[next++];
      facts.set(connection.id, await factsOf(connection, readers, since));
    }
  }
  await Promise.all(Array.from({ length: Math.min(concurrency, queue.length) }, worker));
  return facts;
}

/** The counts of the summary over the grid. */
export interface DashboardSummary {
  total: number;
  working: number;
  /** To repair, to check, paused or blocked. */
  action: number;
  /** The distinct agents that hold a tool of at least one connection read. */
  agents: number;
}

/** The summary of `rows`, its agents counted over the facts read. */
export function dashboardSummary(
  rows: readonly Pick<ConnectionRow, "key" | "status">[],
  facts: ReadonlyMap<string, TileFacts>,
): DashboardSummary {
  const agents = new Set<string>();
  for (const row of rows) for (const id of facts.get(row.key)?.agents ?? []) agents.add(id);
  const action = rows.filter((row) => needsAction(row.status)).length;
  return { total: rows.length, working: rows.length - action, action, agents: agents.size };
}

/** The words of what an account of each provider brings, its services. */
const PROVIDER_BRINGS: Record<ProviderId, string[]> = {
  google: ["connections.tile.brings.mail", "connections.tile.brings.calendar", "connections.tile.brings.drive"],
  microsoft: [
    "connections.tile.brings.mail",
    "connections.tile.brings.calendar",
    "connections.tile.brings.onedrive",
  ],
};

/**
 * The keys of what a connection brings, in words: the services of an
 * account, the mail of a mailbox, the files of a folder; none for a service,
 * whose tools say it.
 */
export function bringsKeys(row: Pick<ConnectionRow, "kind" | "key">): string[] {
  switch (row.kind) {
    case "native": {
      const provider = row.key.split(":")[1];
      return provider === "google" || provider === "microsoft" ? PROVIDER_BRINGS[provider] : [];
    }
    case "imap":
      return ["connections.tile.brings.mail"];
    case "folder":
      return ["connections.tile.brings.files"];
    default:
      return [];
  }
}

/** The agent ids a tile names, Apollia first. */
export function orderedAgents(agents: readonly string[]): string[] {
  return [...agents].sort((a, b) =>
    a === APOLLIA_AGENT_ID ? -1 : b === APOLLIA_AGENT_ID ? 1 : a.localeCompare(b),
  );
}

/** One service of the selection offered by the empty page. */
export interface EmptySuggestion {
  /** The identifier the add window opens it by. */
  entryId: string;
  enrichment: ConnectorEnrichmentView;
}

/** The services of the selection offered by an empty page. */
export const EMPTY_SUGGESTIONS = 3;

/**
 * The first services of the selection, in its order, that are official and
 * carry their logo: what an empty page offers to add.
 */
export function emptySuggestions(
  enrichments: ReadonlyMap<string, ConnectorEnrichmentView>,
  limit: number = EMPTY_SUGGESTIONS,
): EmptySuggestion[] {
  const offered: EmptySuggestion[] = [];
  for (const [identifier, enrichment] of enrichments) {
    if (offered.length >= limit) break;
    if (enrichment.trust_level !== "verified_official" || brandLogoPath(enrichment.icon_name) === null) continue;
    offered.push({ entryId: enrichment.registry_names?.[0] ?? identifier, enrichment });
  }
  return offered;
}
