// ─── Tools, workers and the agentic layer ───

// ─── Enhanced agentic system ────────────────────────────────────

/** Summary of one tool, for the list display. */
export interface ToolSummary {
  /** Unique tool name (for instance "bash_executor"). */
  name: string;
  /** Semver version of the tool. */
  version: string;
  /** Human description of the tool. */
  description: string;
  /** Type d'outil : "native", "mcp", "python". */
  kind: string;
}

/** Detailed view of one tool for introspection (mirrors the Rust ToolDescriptor). */
export interface ToolDescriptorView {
  /** Unique tool name (for instance "bash_executor"). */
  name: string;
  /** Semver version of the tool. */
  version: string;
  /** Human description of the tool. */
  description: string;
  /** Type d'outil : "native", "mcp", "python". */
  kind: string;
  /** Input JSON Schema (`null` when undefined). */
  input_schema: Record<string, unknown> | null;
  /** Output JSON Schema (`null` when undefined). */
  output_schema: Record<string, unknown> | null;
  /** Permissions the tool requires. */
  permissions: string[];
}

/** Statistics of the ORIA plan cache. */
export interface PlanCacheStats {
  /** Total number of cached entries. */
  total_entries: number;
  /** Total number of cache hits since start-up. */
  cache_hits: number;
  /** Total number of cache misses since start-up. */
  cache_misses: number;
  /** Hit rate as a percentage (0-100). */
  hit_rate_pct: number;
  /** RFC 3339 timestamp of the oldest entry (`null` when the cache is empty). */
  oldest_entry_at: string | null;
  /** RFC 3339 timestamp of the newest entry (`null` when the cache is empty). */
  newest_entry_at: string | null;
}

/** Message exchanged between two agents through the mailbox. */
export interface AgentMessage {
  /** Unique message identifier. */
  id: string;
  /** Name of the sending agent. */
  from_agent: string;
  /** Name of the receiving agent. */
  to_agent: string;
  /** Message content (arbitrary JSON). */
  payload: Record<string, unknown>;
  /** Horodatage d'envoi RFC 3339. */
  sent_at: string;
}
