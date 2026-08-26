// ─── Global timeline, session meta-layer, audit trail ───

/** Event of the global timeline. */
export interface GlobalTimelineEvent {
  event_type: "task" | "tool" | "llm" | "hitl" | (string & {});
  timestamp: string;
  summary: string;
  detail: Record<string, unknown>;
}

/** Parameters of the get_global_timeline command. */
export interface TimelineParams {
  window_minutes: number;
}

// ─────────────────────────────────────────────
// Session meta-layer
// ─────────────────────────────────────────────

/** Session event category displayed in the scrubber. */
export type SessionEventKind = "tool" | "memory" | "hitl" | "a2a" | "error";

/** Event persisted for the scrubber and the replay. */
export interface SessionEvent {
  ts: string;
  kind: SessionEventKind;
  label: string;
  correlation_id: string | null;
  payload_json: unknown;
}

/** Entry of the audit trail. */
export interface AuditTrailEntry {
  id: string;
  tool_name: string;
  agent_id: string;
  /** Readable agent name resolved from the registry (for instance "standup-scribe"). */
  agent_name: string;
  timestamp: string;
  duration_ms: number | null;
  exit_code: number | null;
  args_json: string | null;
  stdout: string | null;
  stderr: string | null;
}

/** Daily cost entry per backend. */
export interface LlmDailyCostEntry {
  date: string;
  backend: string;
  cost_usd: number;
}

/** Response of the daily LLM costs. */
export interface LlmDailyCostsResponse {
  entries: LlmDailyCostEntry[];
  days: number;
}

/** Key/value entry of a configuration section. */
export interface ConfigEntry {
  key: string;
  value: string;
}

/** Configuration section grouped by theme. */
export interface ConfigSection {
  name: string;
  description: string;
  entries: ConfigEntry[];
  redirect_route: string | null;
}

/** Flat view of the Apollia OS configuration. */
export interface ApollaConfigView {
  config_path: string;
  config_exists: boolean;
  sections: ConfigSection[];
}

/** System information for the Advanced section of Settings. */
export interface SystemInfo {
  version: string;
  os: string;
  python_path: string | null;
  /** Resolved data directory (`<home>/.apollia`), null when the home is not found. */
  data_dir: string | null;
}

/** OS confinement applied to the child processes of the native tools. */
export type ToolSandbox = "linux_namespaces" | "dev_no_sandbox";

/** Execution model of the agent code. */
export type AgentExecution = "in_process_trusted";

/** Active security posture, mirrored from apollia-core. */
export interface SecurityPosture {
  platform: string;
  tool_sandbox: ToolSandbox;
  agent_execution: AgentExecution;
  rlimits_active: boolean;
  unshare_available: boolean;
}
