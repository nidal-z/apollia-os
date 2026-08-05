/**
 * Runtime action controller for the Agents surface.
 *
 * Owns the per-entity `busy` flags and the start / stop toggles for both single
 * agents and packages. Exposed as a rune-based factory so the sidebar rows, the
 * detail header, and the settings tab all read the same live `busyKeys` map
 * without re-deriving it. Errors surface through the humanized `reportError`
 * toast; domain messages (missing path, partial package failures) use i18n.
 */
import { get } from "svelte/store";
import { t } from "svelte-i18n";
import { agents } from "$lib/stores/agents";
import { triggers } from "$lib/stores/sse";
import {
  packageRuntimeState,
  startPackage,
  stopPackage,
} from "$lib/stores/agentPackages";
import { addToast } from "$lib/components/ui/toast/store";
import { reportError } from "$lib/errors/reportError";
import {
  clearAgentMemory,
  startAgent,
  stopAgent,
  uninstallAgent,
} from "$lib/ipc/agents";
import { isActive } from "./agentStatus";
import type { AgentListItem, AgentPackageListItem } from "$lib/types";

export interface AgentActions {
  readonly busyKeys: Record<string, boolean>;
  isBusy(key: string): boolean;
  toggleAgentRuntime(a: AgentListItem): Promise<void>;
  togglePackageRuntime(pkg: AgentPackageListItem): Promise<void>;
  uninstall(a: AgentListItem, deleteMemory: boolean): Promise<void>;
}

export function createAgentActions(): AgentActions {
  let busyKeys = $state<Record<string, boolean>>({});

  function setBusy(key: string, value: boolean): void {
    busyKeys = { ...busyKeys, [key]: value };
  }

  async function toggleAgentRuntime(a: AgentListItem): Promise<void> {
    const key = `agent:${a.name}`;
    if (busyKeys[key]) return;
    setBusy(key, true);
    try {
      if (isActive(a)) {
        if (a.id) await stopAgent(a.id);
      } else if (a.install_path) {
        await startAgent(a.install_path);
      } else {
        addToast(
          get(t)("agents.start_missing_path", { values: { name: a.name } }),
          "error",
        );
      }
    } catch (err) {
      reportError(err, { surface: "toast" });
    } finally {
      setBusy(key, false);
    }
  }

  async function togglePackageRuntime(pkg: AgentPackageListItem): Promise<void> {
    const key = `pkg:${pkg.name}`;
    if (busyKeys[key]) return;
    const agentsSnap = get(agents);
    const triggersSnap = get(triggers);
    const state = packageRuntimeState(pkg, agentsSnap, triggersSnap);
    setBusy(key, true);
    try {
      const result =
        state.status === "running" || state.status === "partial"
          ? await stopPackage(pkg, agentsSnap, triggersSnap)
          : await startPackage(pkg, agentsSnap, triggersSnap);
      if (result.errors.length > 0) {
        addToast(
          get(t)("agents.package_errors", {
            values: {
              name: pkg.name,
              count: result.errors.length,
              first: result.errors[0],
            },
          }),
          "error",
        );
      }
    } catch (err) {
      reportError(err, { surface: "toast" });
    } finally {
      setBusy(key, false);
    }
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
          await clearAgentMemory(a.memory_namespace ?? a.name);
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
    toggleAgentRuntime,
    togglePackageRuntime,
    uninstall,
  };
}
