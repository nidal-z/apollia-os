/**
 * The sources the "useful connection" step of the first launch reads from:
 * the recorded manifest of the package, the connections of this machine, and
 * the selection the add dialog opens on.
 */
import { fetchMcpCurated, listConnections } from "$lib/ipc/connections";
import { getAgentPackageDetail } from "$lib/ipc/library";
import type { ConnectionSources } from "./connectionStep";

export const connectionSources: ConnectionSources = {
  manifest: async (name) => (await getAgentPackageDetail(name)).manifest,
  connections: listConnections,
  selection: fetchMcpCurated,
};
