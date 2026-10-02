import { writable } from "svelte/store";
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

/** Overview of the aggregated availability of one package. */
export interface PackageRuntimeState {
  totalAgents: number;
  availableAgents: number;
  totalTriggers: number;
  enabledTriggers: number;
  /**
   * "available" when every agent can take a task, "unavailable" when none
   * can, "partial" otherwise. An agent is never started nor stopped: only one
   * switched off from the command line is unavailable.
   */
  status: "available" | "unavailable" | "partial";
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
  const available = pkgAgents.filter((a) => a.installed_at === null || a.enabled).length;
  const enabled = pkgTriggers.filter((t) => t.enabled).length;
  let status: PackageRuntimeState["status"];
  if (available === 0) {
    status = "unavailable";
  } else if (available === pkgAgents.length) {
    status = "available";
  } else {
    status = "partial";
  }
  return {
    totalAgents: pkgAgents.length,
    availableAgents: available,
    totalTriggers: pkgTriggers.length,
    enabledTriggers: enabled,
    status,
  };
}
