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
  capture_llm_prompts: boolean;
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
