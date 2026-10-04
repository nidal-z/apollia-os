import { writable } from "svelte/store";
import { invoke } from "@tauri-apps/api/core";
import type {
  AgentPackageListItem,
  PackagePreview,
  InstallPackageResponse,
  TriggerConfigOverride,
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
