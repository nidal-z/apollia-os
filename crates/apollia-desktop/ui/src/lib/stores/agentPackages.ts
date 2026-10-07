import { writable } from "svelte/store";
import { invoke } from "@tauri-apps/api/core";
import type { AgentPackageListItem } from "$lib/types";

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
