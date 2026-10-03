/**
 * Typed Tauri IPC wrappers behind the adjustment of an agent on its sheet.
 *
 * Thin wrappers over the commands of
 * `crates/apollia-desktop/src/commands/agent_adjust.rs`, each of which calls
 * one runtime route: an override written or removed, the return to defaults,
 * the model assigned or cleared, the presets to choose from. Every write is
 * recorded by the runtime in the history of the agent.
 */
import { invoke } from "@tauri-apps/api/core";
import type { ModelSource, ResolvedAgent } from "$lib/ipc/agentSheet";

/** A preset of a model, as `GET /api/v1/llm/presets` lists it. */
export interface ModelPreset {
  id: number;
  name: string;
  model_source: ModelSource;
  model_id: string;
  is_model_default: boolean;
}

/** Writes the user's `value` of `field` for the agent `name`. */
export async function setAgentOverride(
  name: string,
  field: string,
  value: unknown,
): Promise<ResolvedAgent> {
  return invoke<ResolvedAgent>("set_agent_override", { name, field, value });
}

/** Removes the user's value of `field`: the package's applies again. */
export async function removeAgentOverride(name: string, field: string): Promise<ResolvedAgent> {
  return invoke<ResolvedAgent>("remove_agent_override", { name, field });
}

/** Back to defaults: every override of `name` and its assigned model removed. */
export async function resetAgentOverrides(name: string): Promise<ResolvedAgent> {
  return invoke<ResolvedAgent>("reset_agent_overrides", { name });
}

/** Assigns a model, and optionally one of its presets, to `agent`. */
export async function setAgentModel(
  agent: string,
  modelSource: ModelSource,
  modelId: string,
  presetId: number | null,
): Promise<void> {
  await invoke<unknown>("set_agent_model", { agent, modelSource, modelId, presetId });
}

/** Removes the model assigned to `agent`: it follows the default again. */
export async function clearAgentModel(agent: string): Promise<void> {
  await invoke<unknown>("clear_agent_model", { agent });
}

/** The presets of every model. */
export async function listModelPresets(): Promise<ModelPreset[]> {
  return invoke<ModelPreset[]>("list_model_presets");
}
