/**
 * Pure status helpers for the Agents surface.
 *
 * These map an `AgentListItem` runtime status (or a `PackageRuntimeState`) onto
 * the display tokens the UI needs: a human label, a badge tone, a status-dot
 * color, and the short class label. Kept free of Svelte / i18n coupling (the
 * translator is injected) so they are unit-testable in isolation.
 */
import type { AgentListItem } from "$lib/types";
import type { PackageRuntimeState } from "$lib/stores/agentPackages";

/** Minimal translator shape: key (+ optional interpolation) in, string out. */
export type Translate = (
  key: string,
  options?: { values?: Record<string, string | number> },
) => string;

/** Badge / dot tone shared by agents and packages. */
export type StatusTone = "success" | "warning" | "neutral";

/** True when the agent is running (active or degraded). */
export function isActive(a: AgentListItem): boolean {
  return a.runtime_status === "active" || a.runtime_status === "degraded";
}

/** True when the agent is idle (stopped or never loaded). */
export function isIdle(a: AgentListItem): boolean {
  return a.runtime_status === "stopped" || a.runtime_status === null;
}

/** Human-readable runtime status label. */
export function statusLabel(a: AgentListItem, t: Translate): string {
  switch (a.runtime_status) {
    case "active":
      return t("agents.status_active");
    case "degraded":
      return t("agents.status_degraded");
    case "initializing":
      return t("agents.status_initializing");
    case "stopping":
      return t("agents.status_stopping");
    case "stopped":
      return t("agents.status_stopped");
    default:
      return t("agents.status_not_loaded");
  }
}

/** Badge tone for the runtime status. */
export function statusTone(a: AgentListItem): StatusTone {
  if (a.runtime_status === "active") return "success";
  if (a.runtime_status === "degraded") return "warning";
  return "neutral";
}

/** Status-dot color as a token reference. */
export function statusColor(a: AgentListItem): string {
  if (a.runtime_status === "active") return "hsl(var(--success))";
  if (a.runtime_status === "degraded") return "hsl(var(--warning))";
  return "hsl(var(--muted-foreground))";
}

/** Short user-facing label for the Python agent class. */
export function agentClassLabel(a: AgentListItem): string | null {
  switch (a.agent_class) {
    case "ReActAgent":
      return "Direct";
    case "ConversationalAgent":
      return "Conversational";
    case "OrchestratedAgent":
      return "Orchestrated";
    case "WorkerAgent":
      return "Worker";
    default:
      return a.agent_class ?? null;
  }
}

/** Badge tone for an aggregated package state. */
export function packageStatusTone(s: PackageRuntimeState): StatusTone {
  if (s.status === "running") return "success";
  if (s.status === "partial") return "warning";
  return "neutral";
}

/** Human-readable label for an aggregated package state. */
export function packageStatusLabel(s: PackageRuntimeState, t: Translate): string {
  if (s.status === "running") return t("agents.status_active");
  if (s.status === "partial") return t("agents.pkg_status_partial");
  return t("agents.status_stopped");
}

/** Status-dot color for an aggregated package state. */
export function packageStatusColor(s: PackageRuntimeState): string {
  if (s.status === "running") return "hsl(var(--success))";
  if (s.status === "partial") return "hsl(var(--warning))";
  return "hsl(var(--muted-foreground))";
}
