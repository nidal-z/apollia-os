/**
 * Typed Tauri IPC wrappers behind the Library screen: the installed packages,
 * one package in full, and its uninstallation.
 *
 * Thin wrappers over `crates/apollia-desktop/src/commands/agent_packages.rs`.
 * The uninstallation takes the one path every caller takes: it removes the
 * agents, workflows, tools, automations and brought connections of the
 * package, and erases the memory and the history of its agents only when asked.
 */
import { invoke } from "@tauri-apps/api/core";
import type { AgentPackageDetailView, AgentPackageListItem } from "$lib/types";

/** What an uninstallation removed and kept, as the runtime reported it. */
export interface PackageUninstallReport {
  name: string;
  version: string;
  agents: string[];
  workflows: string[];
  tools: string[];
  triggersRemoved: string[];
  mcpServersRemoved: string[];
  connectionsKept: string[];
  memoryCleared: boolean;
  historyCleared: boolean;
  /** What could not be removed, one line each. */
  leftovers: string[];
}

/** The installed packages, with their publisher, signature and counts. */
export async function listAgentPackages(): Promise<AgentPackageListItem[]> {
  return invoke<AgentPackageListItem[]>("list_agent_packages");
}

/** The installed package `name`: its content by kind and its automations. */
export async function getAgentPackageDetail(name: string): Promise<AgentPackageDetailView> {
  return invoke<AgentPackageDetailView>("get_agent_package_detail", { name });
}

/**
 * Uninstalls the package `name`. With `eraseData`, the memory and the history
 * of its agents are erased too; otherwise they are kept.
 */
export async function uninstallAgentPackage(
  name: string,
  eraseData: boolean,
): Promise<PackageUninstallReport> {
  return invoke<PackageUninstallReport>("uninstall_agent_package", {
    name,
    clearMemory: eraseData,
    clearHistory: eraseData,
  });
}
