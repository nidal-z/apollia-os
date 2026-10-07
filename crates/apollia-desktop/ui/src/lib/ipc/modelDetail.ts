/**
 * Typed IPC wrappers of the panel of a model, on the AI screen: what the
 * panel shows beyond the row of the list, the two overrides of the developer
 * mode, and the presets of a model with their settings.
 *
 * Thin wrappers over the commands of
 * `crates/apollia-desktop/src/commands/ai_model_detail.rs`.
 */
import { invoke } from "@tauri-apps/api/core";
import type { ModelPreset, PresetRequestSettings } from "$lib/ipc/agentAdjust";
import type { ModelCardView, ModelCategory } from "$lib/ipc/modelHub";

export type { PresetRequestSettings };

/** A switch the card of a model declares, in the language it was asked in. */
export type AuthorField = ModelCardView["author_fields"][number];

/** Another weights file of a model, which its source override may name. */
export interface SourceCandidate {
  path: string;
  name: string;
  quantization: string | null;
  size_bytes: number;
}

/** A draft model the load settings of a model may name. */
export interface DraftCandidate {
  /** Identifier in the model index, what the load settings store. */
  id: string;
  /** Parameter count label read from its header, for example `"0.5B"`. */
  parameters: string | null;
  size_bytes: number;
}

/** What the panel of a model shows beyond its row. */
export interface ModelDetail {
  id: string;
  /** Name of the weights file in force: the source override when set. */
  file: string;
  format: "gguf" | "mlx";
  quantization: string | null;
  architecture: string | null;
  /** Category detected from the header. */
  detected_category: ModelCategory;
  /** Category chosen over the detected one. */
  category_override: ModelCategory | null;
  /** The weights file loaded in place of the indexed one. */
  source_file_override: string | null;
  /** The other weights files of the same model on this computer. */
  source_candidates: SourceCandidate[];
  author_fields: AuthorField[];
  /** Whether the engine of the model takes a draft model for speculative decoding. */
  draft_setting: boolean;
  /** The installed draft models whose vocabulary is the one of this model. */
  compatible_drafts: DraftCandidate[];
}

/** A preset with its settings, as the runtime lists it. */
export type PresetDetail = ModelPreset;

/** What a preset is written with. */
export interface PresetWrite {
  name: string;
  load: Record<string, unknown>;
  request: PresetRequestSettings;
  is_model_default: boolean;
}

/** The detail of the installed model `id`, its card read in `lang`. */
export function getInstalledModelDetail(id: string, lang: string): Promise<ModelDetail> {
  return invoke<ModelDetail>("get_installed_model_detail", { id, lang });
}

/** Chooses a category over the detected one, or removes that choice. */
export function setInstalledModelCategoryOverride(
  id: string,
  category: ModelCategory | null,
): Promise<void> {
  return invoke<void>("set_installed_model_category_override", { id, category });
}

/** Loads another weights file of the same model, or removes that choice. */
export function setInstalledModelSourceOverride(id: string, path: string | null): Promise<void> {
  return invoke<void>("set_installed_model_source_override", { id, path });
}

/** Creates a preset of the local model `modelId`. */
export function createModelPreset(modelId: string, preset: PresetWrite): Promise<PresetDetail> {
  return invoke<PresetDetail>("create_model_preset", {
    preset: { ...preset, model_source: "local", model_id: modelId },
  });
}

/** Replaces the name and the settings of the preset `id`. */
export function updateModelPreset(id: number, preset: PresetWrite): Promise<PresetDetail> {
  return invoke<PresetDetail>("update_model_preset", { id, preset });
}
