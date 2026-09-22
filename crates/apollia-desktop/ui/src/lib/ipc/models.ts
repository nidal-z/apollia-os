/**
 * Typed Tauri command wrappers for local model provisioning.
 *
 * Covers the onboarding AI-setup surface: system probing, on-disk GGUF /
 * Whisper scans, curated + HuggingFace downloads, model import, and the STT
 * engine hooks (config, reload, push-to-talk test). Progress is streamed via
 * the `model-download-progress` runtime event, listened to at the call site.
 */
import { invoke } from "@tauri-apps/api/core";

export interface SystemInfo {
  total_ram_gb: number;
  available_ram_gb: number;
  os: string;
  arch: string;
  gpu_available: boolean;
  /** Dedicated accelerator memory in GB, when the engine names a device. */
  gpu_vram_gb: number | null;
}

export interface GgufModelInfo {
  path: string;
  filename: string;
  size_bytes: number;
  size_human: string;
  recommended: boolean;
}

export interface WhisperModelInfo {
  path: string;
  filename: string;
  size_bytes: number;
  model_size: string;
  recommended: boolean;
}

export interface DownloadProgress {
  id: string;
  downloaded_bytes: number;
  total_bytes: number | null;
  speed_bps: number;
  dest_path: string;
  status: "in_progress" | "completed" | "cancelled" | "failed";
}

export interface HfFile {
  filename: string;
  size_bytes: number;
  size_human: string;
  compatibility: "fits" | "might_fit" | "too_large" | null;
  download_url: string;
}

export interface HfModelCard {
  repo_id: string;
  gated: boolean;
  gguf_files: HfFile[];
  compatibility_issue:
    | "embedding_model"
    | "unknown_architecture"
    | "no_gguf_files"
    | null;
}

/** A curated-download or HuggingFace-file request. */
export interface ModelDownloadRequest {
  url: string;
  filename: string;
  repo_id?: string | null;
  /** Destination directory. Defaults to `~/.apollia/models` when absent. */
  dest_dir?: string | null;
}

/** Loose STT config shape: patched field-by-field, never fully retyped here. */
export type SttConfig = Record<string, unknown>;

// ── Recommendations ──────────────────────────────────────────────────────────

/** Why a recommendation sits where it does. Rendered by the i18n layer. */
export type RecommendReason =
  | { reason: "fits_comfortably"; needs_gb: number; budget_gb: number; pool: MemoryPool }
  | { reason: "tight"; needs_gb: number; budget_gb: number; pool: MemoryPool }
  | { reason: "split_across_memory"; gpu_gb: number; vram_gb: number; system_gb: number }
  | { reason: "native_tool_calling" }
  | { reason: "supersedes_generation"; replaces: string }
  | { reason: "trained_context"; tokens: number; asked_tokens: number }
  | { reason: "reduced_context"; tokens: number; default_tokens: number }
  | { reason: "fully_accelerated"; layers: number }
  | { reason: "partial_offload"; gpu_layers: number; total_layers: number }
  | { reason: "sparse_mixture"; active_percent: number }
  | { reason: "quantisation"; format: string; retained_percent: number }
  | { reason: "caveat"; caveat: RecommendCaveat };

/**
 * The memory a model runs from: a discrete card's own memory, one pool shared
 * with the processor (Apple Silicon), or system memory.
 */
export type MemoryPool = "gpu" | "unified" | "system";

/** Something established about a file that the operator should know. */
export type RecommendCaveat =
  | { kind: "no_chat_template" }
  | { kind: "unverified_architecture"; architecture: string; llama_cpp_tag: string }
  | { kind: "header_truncated"; missing: string[] }
  | { kind: "no_pre_tokenizer" };

/** A reason the embedded engine will not run a file. */
export type RecommendBlocker =
  | { kind: "not_generative"; architecture: string }
  | { kind: "unknown_pre_tokenizer"; pre_tokenizer: string; llama_cpp_tag: string }
  | { kind: "sharded_download"; shard_count: number }
  | { kind: "not_gguf"; detail: string };

/** What the compatibility gate concluded about a file. */
export type RecommendVerdict =
  | { status: "supported" }
  | { status: "caveats"; caveats: RecommendCaveat[] }
  | { status: "rejected"; blockers: RecommendBlocker[]; caveats: RecommendCaveat[] };

/** How a model's layers hold their context. */
export interface CacheLayout {
  kind: "dense" | "sliding_window" | "hybrid" | "recurrent";
  full_layers: number;
  window_layers: number;
  recurrent_layers: number;
  full_cells: number;
  window_cells: number;
  bytes: number;
}

/** Where a model's memory goes, in gibibytes. */
export interface MemoryEstimate {
  weights_gb: number;
  kv_cache_gb: number;
  overhead_gb: number;
  total_gb: number;
  /** False when the header lacked the fields the cache formula needs. */
  kv_cache_measured: boolean;
  /** The layout the cache figure came from, when it was measured. */
  layout: CacheLayout | null;
}

/** Where the layers run, and roughly how fast that is. */
export interface OffloadPlan {
  /** The `-ngl` value the runtime will launch with. */
  n_gpu_layers: number;
  total_layers: number;
  device_gb: number;
  host_gb: number;
  fully_offloaded: boolean;
  /** Order-of-magnitude only: for ranking, not for display as a promise. */
  tokens_per_second: number;
}

export interface RecommendedModel {
  file: {
    repo_id: string;
    filename: string;
    download_url: string;
    size_bytes: number;
    gated: boolean;
  };
  family_id: string;
  family_label: string;
  params_b: number;
  /** Quantisation format, when the file name declares one. */
  quant: string | null;
  verdict: RecommendVerdict;
  estimate: MemoryEstimate;
  offload: OffloadPlan;
  badge: "fits" | "might_fit" | "too_large";
  /** Set only when the model fits below the runtime's default context. */
  max_context: number | null;
  score: number;
  reasons: RecommendReason[];
}

export interface HardwareProfileView {
  total_ram_gb: number;
  available_ram_gb: number;
  cpu_model: string;
  cpu_cores: number;
  memory_budget_gb: number;
  accelerator: { kind: string; [key: string]: unknown };
}

/**
 * The three outcomes, kept apart on purpose.
 *
 * `unreachable` is not `ok` with an empty list: the catalogue is live, so no
 * network means no catalogue, and telling the operator their machine runs
 * nothing would be a different and false claim.
 */
export type RecommendOutcome =
  | { status: "ok"; models: RecommendedModel[]; hardware: HardwareProfileView }
  | { status: "unreachable"; detail: string; hardware: HardwareProfileView }
  | { status: "empty"; hardware: HardwareProfileView };

export async function recommendModels(params?: {
  hf_token?: string | null;
  limit?: number;
  n_ctx?: number;
}): Promise<RecommendOutcome> {
  return invoke<RecommendOutcome>("recommend_models", { params: params ?? {} });
}

// ── System + scans ───────────────────────────────────────────────────────────

export async function getAiSetupInfo(): Promise<SystemInfo> {
  return invoke<SystemInfo>("get_ai_setup_info");
}

export async function scanForGgufModels(): Promise<GgufModelInfo[]> {
  return invoke<GgufModelInfo[]>("scan_for_gguf_models");
}

export async function scanForWhisperModels(): Promise<WhisperModelInfo[]> {
  return invoke<WhisperModelInfo[]>("scan_for_whisper_models");
}

// ── LLM setup ────────────────────────────────────────────────────────────────

/**
 * Wire a GGUF file as the `local` backend. `contextWindow` is stored as the
 * backend's `context_window`; `null` keeps the stored value or the default.
 */
export async function setupLocalLlm(
  ggufPath: string,
  contextWindow: number | null = null,
): Promise<void> {
  return invoke<void>("setup_local_llm", { ggufPath, contextWindow });
}

export async function reloadLlm(): Promise<void> {
  return invoke<void>("reload_llm");
}

// ── Downloads ────────────────────────────────────────────────────────────────

export async function startModelDownload(
  request: ModelDownloadRequest,
): Promise<string> {
  return invoke<string>("start_model_download", { request });
}

export async function cancelModelDownload(downloadId: string): Promise<void> {
  return invoke<void>("cancel_model_download", { downloadId });
}

export async function importModelFile(
  sourcePath: string,
  allowedExtensions: string[],
): Promise<string> {
  return invoke<string>("import_model_file", { sourcePath, allowedExtensions });
}

// ── HuggingFace search ───────────────────────────────────────────────────────

export async function searchHfModels(
  query: string,
  limit: number,
): Promise<{ models: HfModelCard[]; next_cursor: string | null }> {
  return invoke<{ models: HfModelCard[]; next_cursor: string | null }>(
    "search_hf_models",
    { params: { query, limit } },
  );
}

export async function getHfModel(repoId: string): Promise<HfModelCard> {
  return invoke<HfModelCard>("get_hf_model", { repoId });
}

// ── STT engine ───────────────────────────────────────────────────────────────

export async function getSttConfig(): Promise<SttConfig> {
  return invoke<SttConfig>("get_stt_config");
}

export async function updateSttConfig(config: SttConfig): Promise<void> {
  return invoke<void>("update_stt_config", { config });
}

export async function setupWhisperModel(
  modelPath: string,
  language: string | undefined,
): Promise<void> {
  return invoke<void>("setup_whisper_model", { modelPath, language });
}

export async function reloadStt(): Promise<void> {
  return invoke<void>("reload_stt");
}

export async function listAudioInputDevices(): Promise<string[]> {
  return invoke<string[]>("list_audio_input_devices");
}

export async function startTourRecording(): Promise<void> {
  return invoke<void>("start_tour_recording");
}

export async function stopTourRecording(): Promise<void> {
  return invoke<void>("stop_tour_recording");
}
