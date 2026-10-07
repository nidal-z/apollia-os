/**
 * Runtime action controller for the Agents surface.
 *
 * Owns the per-agent `busy` flags and the uninstall action. Exposed as a
 * rune-based factory so the sidebar rows and the detail header read the same
 * live `busyKeys` map without re-deriving it. There is no start, stop nor
 * update: an agent is a sheet, available as soon as it is installed, and a
 * newer version arrives with its package. Errors surface through the
 * humanized `reportError` toast.
 */
import { agents } from "$lib/stores/agents";
import { reportError } from "$lib/errors/reportError";
import { clearMemory, uninstallAgent } from "$lib/ipc/agents";
import type { AgentListItem } from "$lib/types";

export interface AgentActions {
  readonly busyKeys: Record<string, boolean>;
  isBusy(key: string): boolean;
  uninstall(a: AgentListItem, deleteMemory: boolean): Promise<void>;
}

export function createAgentActions(): AgentActions {
  let busyKeys = $state<Record<string, boolean>>({});

  function setBusy(key: string, value: boolean): void {
    busyKeys = { ...busyKeys, [key]: value };
  }

  /**
   * Remove an installed agent, optionally taking its memory with it.
   *
   * The command already unregisters the runtime entry, so no stop is issued
   * first. Memory is cleared before the agent goes, because the namespace is
   * addressed by the agent name and there would be nothing left to name it
   * with afterwards. A memory failure is reported and does not cancel the
   * uninstall: the operator asked for the agent to go.
   */
  async function uninstall(a: AgentListItem, deleteMemory: boolean): Promise<void> {
    const key = `agent:${a.name}`;
    if (busyKeys[key]) return;
    setBusy(key, true);
    try {
      if (deleteMemory) {
        try {
          await clearMemory(a.memory_namespace ?? a.name);
        } catch (err) {
          reportError(err, { surface: "toast" });
        }
      }
      await uninstallAgent(a.name);
      // Drop it from the list now rather than waiting for the event round
      // trip, so the row does not linger under a detail pane that no longer
      // has anything to show.
      agents.update((list) => list.filter((x) => x.name !== a.name));
    } catch (err) {
      reportError(err, { surface: "toast" });
    } finally {
      setBusy(key, false);
    }
  }

  return {
    get busyKeys() {
      return busyKeys;
    },
    isBusy: (key: string) => busyKeys[key] === true,
    uninstall,
  };
}
