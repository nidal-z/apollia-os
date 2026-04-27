/**
 * Live agent status snapshot store.
 *
 * Polls `agent_status_snapshot` at a light cadence so the QuickPicker can
 * render accurate `online / busy / offline / error` chips without reloading
 * the full `list_agents` payload.
 */
import { writable, get } from "svelte/store";
import { invoke } from "@tauri-apps/api/core";

export type AgentLiveStatus = "online" | "busy" | "offline" | "error";

export interface AgentStatusSnapshot {
  name: string;
  status: AgentLiveStatus;
}

const POLL_INTERVAL_MS = 3000;

export const agentStatuses = writable<Record<string, AgentLiveStatus>>({});
export const agentStatusLoading = writable<boolean>(false);

let pollTimer: ReturnType<typeof setInterval> | null = null;
let subscribers = 0;

async function refresh(): Promise<void> {
  try {
    const snapshot = await invoke<AgentStatusSnapshot[]>("agent_status_snapshot");
    const next: Record<string, AgentLiveStatus> = {};
    for (const s of snapshot) next[s.name] = s.status;
    agentStatuses.set(next);
  } catch {
    // Swallow — keep last known state. UI shows stale data rather than flicker.
  }
}

/**
 * Attach a polling subscriber. Returns a disposer — the last disposer stops
 * the underlying `setInterval`, so unused pages don't keep polling.
 */
export function startAgentStatusPolling(): () => void {
  subscribers += 1;
  if (subscribers === 1) {
    agentStatusLoading.set(true);
    void refresh().finally(() => agentStatusLoading.set(false));
    pollTimer = setInterval(() => void refresh(), POLL_INTERVAL_MS);
  }
  return () => {
    subscribers = Math.max(0, subscribers - 1);
    if (subscribers === 0 && pollTimer) {
      clearInterval(pollTimer);
      pollTimer = null;
    }
  };
}

export function getAgentStatus(name: string): AgentLiveStatus {
  return get(agentStatuses)[name] ?? "offline";
}
