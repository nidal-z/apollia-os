/**
 * Pure status helpers for the Agents surface.
 *
 * These map an `AgentListItem` onto its availability and the display tokens
 * the UI needs: a human label, a badge tone, a status-dot
 * color, and the short class label. Kept free of Svelte / i18n coupling (the
 * translator is injected) so they are unit-testable in isolation.
 */
import type { AgentListItem } from "$lib/types";

/** Minimal translator shape: key (+ optional interpolation) in, string out. */
export type Translate = (
  key: string,
  options?: { values?: Record<string, string | number> },
) => string;

/** Badge / dot tone of an agent. */
export type StatusTone = "success" | "warning" | "neutral";

/**
 * What the Agents surface shows of an agent: available, degraded, or
 * unavailable.
 *
 * An agent is never started nor stopped from the interface: it is loaded by
 * its first task and released once idle. The runtime states that describe
 * that cycle (stopped, stopping, not loaded yet) all read "available" here.
 * Only an installed agent switched off from the command line is unavailable,
 * and a degraded one stays callable with a warning.
 */
export type Availability = "available" | "degraded" | "unavailable";

/** The availability of an agent. */
export function availability(a: AgentListItem): Availability {
  if (a.installed_at !== null && !a.enabled) return "unavailable";
  if (a.runtime_status === "degraded") return "degraded";
  return "available";
}

/** True when a task can reach the agent. */
export function isAvailable(a: AgentListItem): boolean {
  return availability(a) !== "unavailable";
}

/** Human-readable availability label. */
export function statusLabel(a: AgentListItem, t: Translate): string {
  switch (availability(a)) {
    case "available":
      return t("agents.status_available");
    case "degraded":
      return t("agents.status_degraded");
    default:
      return t("agents.status_unavailable");
  }
}

/** Badge tone for the availability. */
export function statusTone(a: AgentListItem): StatusTone {
  switch (availability(a)) {
    case "available":
      return "success";
    case "degraded":
      return "warning";
    default:
      return "neutral";
  }
}

/** Status-dot color as a token reference. */
export function statusColor(a: AgentListItem): string {
  switch (availability(a)) {
    case "available":
      return "hsl(var(--success))";
    case "degraded":
      return "hsl(var(--warning))";
    default:
      return "hsl(var(--muted-foreground))";
  }
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
