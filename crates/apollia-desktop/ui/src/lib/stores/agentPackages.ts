import { writable, get } from "svelte/store";
import { t } from "svelte-i18n";
import { invoke } from "@tauri-apps/api/core";
import type {
  AgentPackageListItem,
  AgentPackageDetailView,
  AgentListItem,
  PackagePreview,
  InstallPackageResponse,
  TriggerConfigOverride,
  TriggerStatus,
} from "$lib/types";

export const agentPackages = writable<AgentPackageListItem[]>([]);
export const packagesLoading = writable(false);
export const packagesError = writable<string | null>(null);

export async function refreshPackages(): Promise<void> {
  packagesLoading.set(true);
  packagesError.set(null);
  try {
    const pkgs = await invoke<AgentPackageListItem[]>("list_agent_packages");
    agentPackages.set(pkgs);
  } catch (e) {
    packagesError.set(String(e));
  } finally {
    packagesLoading.set(false);
  }
}

export async function previewPackage(path: string): Promise<PackagePreview> {
  return invoke<PackagePreview>("preview_agent_package", { path });
}

/**
 * Installs a package from a local path.
 *
 * When `depsConfirmed` is `false` and the manifest declares pip packages, the
 * backend returns the error `DEPS_CONFIRMATION_REQUIRED:<n>:<csv>`, which the
 * UI has to parse to show an explicit confirmation step.
 */
export async function installPackage(
  path: string,
  triggerConfigs: TriggerConfigOverride[] = [],
  depsConfirmed: boolean = false,
): Promise<InstallPackageResponse> {
  const result = await invoke<InstallPackageResponse>("install_agent_package", {
    path,
    triggerConfigs,
    depsConfirmed,
  });
  await refreshPackages();
  return result;
}

export async function uninstallPackage(name: string): Promise<void> {
  await invoke("uninstall_agent_package", { name });
  agentPackages.update((pkgs) => pkgs.filter((p) => p.name !== name));
}

export async function getPackageDetail(name: string): Promise<AgentPackageDetailView> {
  return invoke<AgentPackageDetailView>("get_agent_package_detail", { name });
}

/** Overview of the aggregated runtime state of one package. */
export interface PackageRuntimeState {
  totalAgents: number;
  runningAgents: number;
  totalTriggers: number;
  enabledTriggers: number;
  /** "running" when every agent is active, "stopped" when none runs, "partial" otherwise. */
  status: "running" | "stopped" | "partial";
}

/** Computes the aggregated state of a package from the agent and trigger snapshots. */
export function packageRuntimeState(
  pkg: AgentPackageListItem,
  agentsSnap: AgentListItem[],
  triggersSnap: TriggerStatus[],
): PackageRuntimeState {
  const names = new Set(pkg.agents.map((a) => a.name));
  const pkgAgents = agentsSnap.filter((a) => names.has(a.name));
  const pkgTriggers = triggersSnap.filter((t) => names.has(t.agent));
  const running = pkgAgents.filter(
    (a) => a.runtime_status === "active" || a.runtime_status === "degraded",
  ).length;
  const enabled = pkgTriggers.filter((t) => t.enabled).length;
  let status: PackageRuntimeState["status"];
  if (pkgAgents.length === 0) {
    status = "stopped";
  } else if (running === 0) {
    status = "stopped";
  } else if (running === pkgAgents.length) {
    status = "running";
  } else {
    status = "partial";
  }
  return {
    totalAgents: pkgAgents.length,
    runningAgents: running,
    totalTriggers: pkgTriggers.length,
    enabledTriggers: enabled,
    status,
  };
}

/**
 * Starts every dependency of a package, in this order:
 *   1. triggers (activation)
 *   2. workers
 *   3. assistants / director
 *
 * Individual errors are collected and returned without interrupting the sequence.
 */
export async function startPackage(
  pkg: AgentPackageListItem,
  agentsSnap: AgentListItem[],
  triggersSnap: TriggerStatus[],
): Promise<{ errors: string[] }> {
  const errors: string[] = [];
  const names = new Set(pkg.agents.map((a) => a.name));

  // 1. Triggers
  for (const tr of triggersSnap.filter((t) => names.has(t.agent))) {
    if (tr.enabled) continue;
    try {
      await invoke("set_trigger_enabled", { id: tr.id, enabled: true });
    } catch (e) {
      errors.push(
        get(t)("agents.package_start_errors.trigger", {
          values: { id: tr.id, error: String(e) },
        }),
      );
    }
  }

  // 2. Workers, then 3. Assistants / director - two ordered passes.
  const isRunning = (a: AgentListItem) =>
    a.runtime_status === "active" || a.runtime_status === "degraded";

  const pkgRoles = new Map(pkg.agents.map((a) => [a.name, a.role] as const));
  const ordered = agentsSnap
    .filter((a) => names.has(a.name))
    .sort((x, y) => {
      const rx = pkgRoles.get(x.name) === "worker" ? 0 : 1;
      const ry = pkgRoles.get(y.name) === "worker" ? 0 : 1;
      return rx - ry;
    });

  for (const agent of ordered) {
    if (isRunning(agent)) continue;
    if (!agent.install_path) {
      errors.push(
        get(t)("agents.package_start_errors.missing_install_path", {
          values: { name: agent.name },
        }),
      );
      continue;
    }
    try {
      await invoke("start_agent", { path: agent.install_path });
    } catch (e) {
      errors.push(`${agent.name} : ${String(e)}`);
    }
  }

  return { errors };
}

/**
 * Stops every dependency of a package, in the reverse order:
 *   1. assistants / director
 *   2. workers
 *   3. triggers (disabling)
 */
export async function stopPackage(
  pkg: AgentPackageListItem,
  agentsSnap: AgentListItem[],
  triggersSnap: TriggerStatus[],
): Promise<{ errors: string[] }> {
  const errors: string[] = [];
  const names = new Set(pkg.agents.map((a) => a.name));

  const isRunning = (a: AgentListItem) =>
    a.runtime_status === "active" || a.runtime_status === "degraded";

  const pkgRoles = new Map(pkg.agents.map((a) => [a.name, a.role] as const));
  const ordered = agentsSnap
    .filter((a) => names.has(a.name))
    .sort((x, y) => {
      // Assistants/director first, then workers.
      const rx = pkgRoles.get(x.name) === "worker" ? 1 : 0;
      const ry = pkgRoles.get(y.name) === "worker" ? 1 : 0;
      return rx - ry;
    });

  for (const agent of ordered) {
    if (!isRunning(agent) || !agent.id) continue;
    try {
      await invoke("stop_agent", { agentId: agent.id });
    } catch (e) {
      errors.push(`${agent.name} : ${String(e)}`);
    }
  }

  for (const tr of triggersSnap.filter((t) => names.has(t.agent))) {
    if (!tr.enabled) continue;
    try {
      await invoke("set_trigger_enabled", { id: tr.id, enabled: false });
    } catch (e) {
      errors.push(
        get(t)("agents.package_start_errors.trigger", {
          values: { id: tr.id, error: String(e) },
        }),
      );
    }
  }

  return { errors };
}
