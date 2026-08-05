/**
 * Typed Tauri IPC wrappers for the observability *policy* domain.
 *
 * One thin function per `#[tauri::command]` in
 * `crates/apollia-desktop/src/commands/observability.rs`. The Settings >
 * Observability sub-page calls these instead of `invoke()` directly, so the
 * command names and the payload shape live in a single place.
 *
 * The shape mirrors `ObservabilityConfig` (apollia-core), the `[observability]`
 * section of `apollia.toml`. Changes persisted via `setObservabilityConfig`
 * apply on the next runtime restart (no live reload).
 */
import { invoke } from "@tauri-apps/api/core";

/** The `[observability]` policy section of `apollia.toml`. All 10 fields. */
export interface ObservabilityConfig {
  /** Persist the LLM 'thought' field at each ReAct turn. */
  capture_thoughts: boolean;
  /** Persist raw prompts and responses. Sensitive. */
  /** Persist the args JSON of each tool invocation. */
  capture_tool_args: boolean;
  /** Persist the output JSON of each tool invocation. */
  capture_tool_outputs: boolean;
  /** Persist debug/info/warn/error messages emitted by agents. */
  capture_agent_logs: boolean;
  /** Write `prompt_text` to `llm_calls.db`. Sensitive, off by default. */
  debug_log_prompt: boolean;
  /** Maximum size (bytes) of task/step inputs kept before truncation. */
  max_input_bytes: number;
  /** Maximum size (bytes) of outputs/completions kept before truncation. */
  max_output_bytes: number;
  /** Maximum size (bytes) of a tool's stdout/stderr kept before truncation. */
  max_tool_output_bytes: number;
  /** Number of days events are kept before automatic purge. */
  retention_days: number;
}

/** Fetch the active observability policy from `apollia.toml`. */
export async function getObservabilityConfig(): Promise<ObservabilityConfig> {
  return invoke<ObservabilityConfig>("get_observability_config");
}

/**
 * Persist the observability policy to `apollia.toml`. The runtime does not
 * hot-reload this section: the new policy applies on the next app restart.
 */
export async function setObservabilityConfig(
  config: ObservabilityConfig,
): Promise<void> {
  return invoke<void>("set_observability_config", { config });
}

// ───────────────────────────────────────────────────────────────────────────
// Audit trail aggregates
// ───────────────────────────────────────────────────────────────────────────

/**
 * Journal-wide counters served by `GET /api/v1/audit/stats`.
 *
 * These three fields are the whole response body: the route answers with
 * `AuditStatsResponse { total_events, unique_tools, unique_agents }`. They cover
 * the entire trail, unlike the per-page counters the audit table computes from
 * the rows it happens to have loaded.
 */
export interface AuditStatsSummary {
  /** Tool invocations recorded in the trail, all time. */
  total_events: number;
  /** Distinct tool names that appear in the trail. */
  unique_tools: number;
  /** Distinct agents that appear in the trail. */
  unique_agents: number;
}

/** Reads one non-negative counter from a raw JSON object, `0` when absent. */
function counter(source: Record<string, unknown>, key: string): number {
  const value = source[key];
  return typeof value === "number" && Number.isFinite(value) ? value : 0;
}

/**
 * Narrows the raw `get_audit_stats` payload.
 *
 * The command relays `serde_json::Value` untouched, so nothing guarantees the
 * shape at the type level. A missing counter reads as `0` rather than `NaN`.
 * Exported for unit tests.
 */
export function normalizeAuditStats(raw: unknown): AuditStatsSummary {
  if (typeof raw !== "object" || raw === null) {
    return { total_events: 0, unique_tools: 0, unique_agents: 0 };
  }
  const source = raw as Record<string, unknown>;
  return {
    total_events: counter(source, "total_events"),
    unique_tools: counter(source, "unique_tools"),
    unique_agents: counter(source, "unique_agents"),
  };
}

/** Reads the journal-wide audit counters. */
export async function getAuditStats(): Promise<AuditStatsSummary> {
  return normalizeAuditStats(await invoke<unknown>("get_audit_stats"));
}

// ───────────────────────────────────────────────────────────────────────────
// Lifecycle hooks
// ───────────────────────────────────────────────────────────────────────────

/**
 * One lifecycle hook handler registered at runtime startup.
 *
 * Mirrors `HookHandlerSummary` served by `GET /api/v1/hooks`, relayed as raw
 * JSON by the `get_active_hooks` command. The registry is built once from
 * `apollia.toml` and never mutates afterwards: there is no dynamic
 * registration and no hot reload, so this list is a startup fact.
 */
export interface ActiveHook {
  /** Zero-based index in the configuration, in declaration order. */
  id: number;
  /** Delivery mechanism: `"command"` or `"http"`. */
  type: string;
  /** Lifecycle events the handler subscribes to, snake_case wire names. */
  events: string[];
  /** Configured timeout in milliseconds. */
  timeout_ms: number;
  /** Command argv joined by spaces, or the URL for an http handler. */
  target: string;
}

/**
 * Narrows one raw entry of the `get_active_hooks` payload.
 *
 * Exported for unit tests.
 */
export function isActiveHook(raw: unknown): raw is ActiveHook {
  if (typeof raw !== "object" || raw === null) return false;
  const candidate = raw as Record<string, unknown>;
  return (
    typeof candidate.id === "number" &&
    typeof candidate.type === "string" &&
    Array.isArray(candidate.events) &&
    candidate.events.every((event) => typeof event === "string") &&
    typeof candidate.timeout_ms === "number" &&
    typeof candidate.target === "string"
  );
}

/**
 * Keeps the entries that match the documented shape.
 *
 * An empty array is a valid configuration (no hook declared), so it is a clean
 * state rather than an error. Exported for unit tests.
 */
export function normalizeActiveHooks(raw: unknown): ActiveHook[] {
  return Array.isArray(raw) ? raw.filter(isActiveHook) : [];
}

/** Reads the lifecycle hook handlers registered at startup, declaration order. */
export async function getActiveHooks(): Promise<ActiveHook[]> {
  return normalizeActiveHooks(await invoke<unknown>("get_active_hooks"));
}
