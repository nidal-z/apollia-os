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
}

/** Loose STT config shape: patched field-by-field, never fully retyped here. */
export type SttConfig = Record<string, unknown>;

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

export async function setupLocalLlm(ggufPath: string): Promise<void> {
  return invoke<void>("setup_local_llm", { ggufPath });
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
