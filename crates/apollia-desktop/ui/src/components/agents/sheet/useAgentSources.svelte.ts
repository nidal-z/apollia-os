/**
 * The registry of sources as an agent's sheet reads it: every source, and
 * where the index of each one that points at documents stands.
 *
 * Owned by the sheet, which hands it to its "Sources" section in reading and
 * in adjustment. `refresh` reads the registry and the indexes again; the
 * section calls it while a source is still being read.
 */
import {
  addSource,
  getSourceIndex,
  listSources,
  removeSource,
  type NewSource,
  type Source,
  type SourceIndex,
} from "$lib/ipc/sources";
import { indexedSources } from "./agentSources";

export interface AgentSources {
  /** Every source of the registry, oldest first. */
  readonly registry: Source[];
  /** The index of each source that points at documents, by identifier. */
  readonly indexes: Record<string, SourceIndex>;
  /** Why the registry could not be read, as the runtime phrased it. */
  readonly error: string | null;
  refresh(): Promise<void>;
  /** Adds a source; `null` when the runtime refused, `error` saying why. */
  add(source: NewSource): Promise<Source | null>;
  /** Removes a source and its index; whether it went. */
  remove(id: string): Promise<boolean>;
}

export function createAgentSources(): AgentSources {
  let registry = $state<Source[]>([]);
  let indexes = $state<Record<string, SourceIndex>>({});
  let error = $state<string | null>(null);
  let ticket = 0;

  function message(err: unknown): string {
    return err instanceof Error ? err.message : String(err);
  }

  async function refresh(): Promise<void> {
    const mine = ++ticket;
    try {
      const listed = await listSources();
      const read = await Promise.all(
        indexedSources(listed).map((source) => getSourceIndex(source.id).catch(() => null)),
      );
      if (mine !== ticket) return;
      const next: Record<string, SourceIndex> = {};
      for (const index of read) if (index) next[index.id] = index;
      registry = listed;
      indexes = next;
      error = null;
    } catch (err) {
      if (mine === ticket) error = message(err);
    }
  }

  return {
    get registry() {
      return registry;
    },
    get indexes() {
      return indexes;
    },
    get error() {
      return error;
    },
    refresh,
    async add(source) {
      try {
        const added = await addSource(source);
        await refresh();
        return added;
      } catch (err) {
        error = message(err);
        return null;
      }
    },
    async remove(id) {
      try {
        await removeSource(id);
        await refresh();
        return true;
      } catch (err) {
        error = message(err);
        return false;
      }
    },
  };
}
