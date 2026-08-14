/**
 * Visibility predicate for the Start button of the Settings tab execution
 * section. Extracted from AgentSettingsTab.svelte so the component test can
 * exercise the exact expression the template renders: importing the component
 * itself pulls the theme store, which touches localStorage at module scope.
 */
import type { AgentListItem } from "$lib/types";
import { isIdle } from "../agentStatus";

/**
 * Start is offered whenever the agent is idle, installed, and has an install
 * path to launch from. Idle means stopped or never loaded: after Stop the
 * registry keeps the entry and reports "stopped", never null, so testing
 * `runtime_status === null` here left a stopped agent unrestartable until the
 * app restarted.
 */
export function showStartButton(agent: AgentListItem): boolean {
  return (
    isIdle(agent) && agent.installed_at !== null && agent.install_path !== null
  );
}
