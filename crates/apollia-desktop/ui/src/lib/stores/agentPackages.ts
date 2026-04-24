import { writable } from "svelte/store";
import { invoke } from "@tauri-apps/api/core";
import type {
  AgentPackageListItem,
  AgentPackageDetailView,
  PackagePreview,
  InstallPackageResponse,
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

export async function installPackage(path: string): Promise<InstallPackageResponse> {
  const result = await invoke<InstallPackageResponse>("install_agent_package", { path });
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
