// Typed IPC wrappers for the lifecycle hooks view.
//
// `getActiveHooks` lists the handlers registered at startup (read-only).
// PreToolUse decisions are not persisted: they stream live on the dedicated
// `"hook-decision"` Tauri event, which the decision log accumulates in memory.

import { invoke } from "@tauri-apps/api/core";

/** Lifecycle events a hook handler can subscribe to (snake_case wire names). */
export type HookEvent =
  | "pre_tool_use"
  | "post_tool_use"
  | "pre_compact"
  | "post_compact"
  | "subagent_start"
  | "subagent_stop";

/**
 * A registered lifecycle hook handler.
 *
 * Mirrors the runtime `HookHandlerSummary` returned by `GET /api/v1/hooks`: one
 * handler can subscribe to several events, hence `events` is a list rather than
 * a single type.
 */
export interface HookHandler {
  id: number;
  type: "command" | "http";
  events: HookEvent[];
  timeout_ms: number;
  target: string;
}

/** Aggregate PreToolUse decision kind. */
export type HookDecisionKind = "allow" | "deny" | "rewrite";

/** Payload of the live `"hook-decision"` Tauri event (one per resolved call). */
export interface HookDecisionPayload {
  run_id: string;
  session_id: string;
  tool_name: string;
  decision: HookDecisionKind;
  /** Replacement arguments as a JSON string, present only for `"rewrite"`. */
  rewritten_args: string | null;
}

/** Lists the lifecycle hook handlers registered for the running agent. */
export async function getActiveHooks(): Promise<HookHandler[]> {
  return invoke<HookHandler[]>("get_active_hooks");
}
