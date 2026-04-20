/**
 * Local telemetry sink for permission-related UX events (US-SP42-054).
 *
 * Events never leave the device. They are posted to the `telemetry_emit`
 * Tauri command when available and silently dropped otherwise — the UI
 * must keep working even when the backend is older than this build.
 */
import { invoke } from "@tauri-apps/api/core";

export type AlwaysAcceptScope =
  | "session_only"
  | "agent_tool_pair"
  | "tool_global"
  | "all_always";

export interface AlwaysAcceptTelemetry {
  scope: AlwaysAcceptScope;
  tool: string;
  agent_kind?: string;
}

export async function emitAlwaysAcceptScope(ev: AlwaysAcceptTelemetry): Promise<void> {
  try {
    await invoke("telemetry_emit", {
      event: "always_accept_scope_chosen",
      payload: ev,
    });
  } catch {
    // Swallow: telemetry is strictly best-effort.
  }
}

export async function emitPreviewLoaded(kind: string, tool: string): Promise<void> {
  try {
    await invoke("telemetry_emit", {
      event: "permission_preview_loaded",
      payload: { kind, tool },
    });
  } catch {
    // Swallow.
  }
}
