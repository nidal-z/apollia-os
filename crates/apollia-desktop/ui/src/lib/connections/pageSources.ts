/**
 * The sources of the Connections page, read side by side.
 *
 * Each source is settled on its own: one that fails keeps what the page
 * already showed for it and adds one banner, and every other source is still
 * shown. Pure apart from the injected fetchers, so a test hands it stubs.
 */
import type {
  AgentListItem,
  Connection,
  ConnectionReason,
  ConnectionStatusKey,
  ConnectorEnrichmentView,
  McpServerStatusView,
} from "$lib/types";

/** A source of the page, named for its banner. */
export type PageSource = "connections" | "services" | "descriptions" | "agents";

/** The key of the banner of each source that could not be read. */
export const FAILURE_KEYS: Record<PageSource, string> = {
  connections: "connections.list.source_failed.connections",
  services: "connections.list.source_failed.services",
  descriptions: "connections.list.source_failed.descriptions",
  agents: "connections.list.source_failed.agents",
};

/** One source that could not be read, and why. */
export interface SourceFailure {
  source: PageSource;
  message: string;
}

/** What the page shows. */
export interface PageData {
  connections: Connection[];
  servers: McpServerStatusView[];
  enrichments: Map<string, ConnectorEnrichmentView>;
  agents: AgentListItem[];
}

/** How each source is read. */
export interface PageFetchers {
  connections: () => Promise<Connection[]>;
  servers: () => Promise<McpServerStatusView[]>;
  enrichments: () => Promise<{ package_identifier: string; enrichment: ConnectorEnrichmentView }[]>;
  agents: () => Promise<AgentListItem[]>;
}

/** The page before anything was read. */
export const EMPTY_PAGE: PageData = {
  connections: [],
  servers: [],
  enrichments: new Map(),
  agents: [],
};

function messageOf(reason: unknown): string {
  return reason instanceof Error ? reason.message : String(reason);
}

/**
 * Read every source at once. A source that answers replaces its part of
 * `previous`; one that fails keeps it and adds its failure, in the order of
 * the sources.
 */
export async function loadPageSources(
  fetchers: PageFetchers,
  previous: PageData = EMPTY_PAGE,
): Promise<{ data: PageData; failures: SourceFailure[] }> {
  const [connections, servers, enrichments, agents] = await Promise.allSettled([
    fetchers.connections(),
    fetchers.servers(),
    fetchers.enrichments(),
    fetchers.agents(),
  ]);
  const failures: SourceFailure[] = [];
  const kept = <T>(source: PageSource, result: PromiseSettledResult<T>, fallback: T): T => {
    if (result.status === "fulfilled") return result.value;
    failures.push({ source, message: messageOf(result.reason) });
    return fallback;
  };
  const keptConnections = kept("connections", connections, previous.connections);
  const keptServers = kept("services", servers, previous.servers);
  const entries = kept("descriptions", enrichments, null);
  const data: PageData = {
    connections: keptConnections,
    servers: keptServers,
    enrichments:
      entries === null
        ? previous.enrichments
        : new Map(entries.map((e) => [e.package_identifier, e.enrichment])),
    agents: kept("agents", agents, previous.agents),
  };
  return { data, failures };
}
/** A connection found in a new state, as `ConnectionStateChanged` carries it. */
export interface StateChange {
  id: string;
  state: ConnectionStatusKey;
  reason: ConnectionReason | null;
  checked_at: string;
}

/** The connections with the one `change` names in its new state; the others as they were. */
export function applyStateChange(connections: readonly Connection[], change: StateChange): Connection[] {
  return connections.map((c) =>
    c.id === change.id
      ? { ...c, status: change.state, reason: change.reason, checkedAt: change.checked_at }
      : c,
  );
}
