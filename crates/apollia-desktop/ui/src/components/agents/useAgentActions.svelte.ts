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
import type { HumanizedError } from "$lib/errors/humanize";
import {
  clearMemory,
  startAgent,
  stopAgent,
  uninstallAgent,
  updateAgent,
  type UpdateAgentResult,
} from "$lib/ipc/agents";
import { listAgents } from "$lib/ipc/connections";
import { isActive } from "./agentStatus";
import type { AgentListItem, AgentPackageListItem } from "$lib/types";

export interface AgentActions {
  readonly busyKeys: Record<string, boolean>;
  /**
   * Last failed agent update, kept whole rather than reduced to a sentence.
   *
   * Two causes reach it: a Python module the loader refuses, and a runtime
   * instance that could not be cycled onto the new module. Both carry a raw
   * cause the route renders behind a details disclosure. `null` once no update
   * is in error.
   */
  readonly updateError: HumanizedError | null;
  isBusy(key: string): boolean;
  toggleAgentRuntime(a: AgentListItem): Promise<void>;
  togglePackageRuntime(pkg: AgentPackageListItem): Promise<void>;
  update(a: AgentListItem, path: string): Promise<void>;
  clearUpdateError(): void;
  uninstall(a: AgentListItem, deleteMemory: boolean): Promise<void>;
}

export function createAgentActions(): AgentActions {
  let busyKeys = $state<Record<string, boolean>>({});
  let updateError = $state<HumanizedError | null>(null);

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
   * Build the inline banner for a restart the runtime could not complete.
   *
   * Not routed through `humanize`: the raw cause is a stop refusal or a load
   * failure, and both would land in the generic bucket, whose copy says
   * nothing about which version is serving. That is the one thing the operator
   * needs to read here, so the sentence is written for the case and the raw
   * cause stays behind the details disclosure.
   */
  function restartFailure(
    name: string,
    result: UpdateAgentResult,
  ): HumanizedError {
    const stem =
      result.restart_outcome === "stop_failed"
        ? "agents.update_restart_stop_failed"
        : "agents.update_restart_start_failed";
    return {
      title: get(t)("agents.update_restart_failed_title"),
      friendly_message: get(t)(stem, { values: { name } }),
      suggested_action: get(t)(`${stem}_action`),
      category: "generic",
      detail: result.restart_error ?? undefined,
    };
  }

  /**
   * Replace an installed agent's Python file with the one just picked.
   *
   * Reinstalling by hand meant uninstall then install, which loses the
   * auto-start flag and the install date. The command keeps both.
   *
   * Two ways this reports something false if left alone. The loader rejects a
   * malformed module and the Python cause is what tells the operator what to
   * fix: it is kept whole on `updateError`, not flattened to a toast. And a
   * running agent keeps its imported module in memory, so the command cycles
   * it; `restart_outcome` says whether the new version is actually serving,
   * and each of the four cases gets its own sentence. A success toast is only
   * fired for the two that are one.
   */
  async function update(a: AgentListItem, path: string): Promise<void> {
    const key = `agent:${a.name}`;
    if (busyKeys[key]) return;
    setBusy(key, true);
    updateError = null;
    try {
      const result = await updateAgent(a.name, path);
      // Read the list back instead of waiting for the runtime event: the row
      // and the header badge must carry the new version and the post-restart
      // status by the time the operator looks at them. A failed read-back is
      // not an update failure, the event round trip still repairs the list.
      try {
        agents.set(await listAgents());
      } catch {
        // list refresh unavailable, the runtime event will settle it
      }
      if (
        result.restart_outcome === "stop_failed" ||
        result.restart_outcome === "start_failed"
      ) {
        updateError = restartFailure(a.name, result);
        return;
      }
      addToast(
        get(t)(
          result.restart_outcome === "restarted"
            ? "agents.update_success_restarted"
            : "agents.update_success",
          { values: { name: result.name, version: result.version } },
        ),
        "success",
      );
    } catch (err) {
      updateError = reportError(err, { surface: "inline" });
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
    get updateError() {
      return updateError;
    },
    isBusy: (key: string) => busyKeys[key] === true,
    toggleAgentRuntime,
    togglePackageRuntime,
    update,
    clearUpdateError: () => {
      updateError = null;
    },
    uninstall,
  };
}
