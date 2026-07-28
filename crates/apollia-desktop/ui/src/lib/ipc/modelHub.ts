/**
 * Model Hub IPC - typed wrappers over the Tauri commands and events that back
 * Settings > Model Hub (hardware profile, HuggingFace search / detail, GGUF
 * downloads, and the installed-models manager).
 *
 * Every command the sub-page invokes lives here so the Svelte components stay
 * free of raw `invoke(...)` string keys. The shapes mirror the Rust command
 * contract 1:1; keep them in sync when the backend changes.
 */
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

// ── Hardware profile ──────────────────────────────────────────────────────

export interface AcceleratorProfile {
  kind: "none" | "apple_silicon" | "cuda" | "generic";
  chip?: string;
  generation?: number;
  vram_gb?: number;
  device_name?: string;
  compute_capability?: [number, number];
}

export interface HardwareProfile {
  total_ram_gb: number;
  available_ram_gb: number;
  cpu_model: string;
  cpu_cores: number;
  accelerator: AcceleratorProfile;
  memory_budget_gb: number;
}

// ── HuggingFace model cards ───────────────────────────────────────────────

export type Compatibility = "fits" | "might_fit" | "too_large" | null;

export interface HfFile {
  filename: string;
  size_bytes: number;
  size_human: string;
  compatibility: Compatibility;
  download_url: string;
}

export interface GenerationConfig {
  temperature?: number;
  top_p?: number;
  top_k?: number;
  repetition_penalty?: number;
  max_new_tokens?: number;
}

export type CompatIssue =
  | "embedding_model"
  | "unknown_architecture"
  | "no_gguf_files"
  | null;

export interface HfModelCard {
  repo_id: string;
  author: string | null;
  downloads: number;
  likes: number;
  license: string | null;
  tags: string[];
  gated: boolean;
  total_size_bytes: number | null;
  gguf_files: HfFile[];
  generation_config: GenerationConfig | null;
  compatibility_issue: CompatIssue;
  model_type: string | null;
}

export interface SearchParams {
  query: string;
  limit: number;
  sort: string;
  pipeline_tag: string;
  language: string | null;
  next_cursor: string | null;
  hf_token: string | null;
}

export interface SearchPage {
  models: HfModelCard[];
  next_cursor: string | null;
}

// ── Downloads ─────────────────────────────────────────────────────────────

export type DownloadStatus =
  | "in_progress"
  | "completed"
  | "cancelled"
  | "failed";

export interface DownloadProgress {
  id: string;
  downloaded_bytes: number;
  total_bytes: number | null;
  speed_bps: number;
  dest_path: string;
  status: DownloadStatus;
}

export interface DownloadRequest {
  url: string;
  filename: string;
  hf_token: string | null;
  repo_id: string | null;
}

// ── Installed models ──────────────────────────────────────────────────────

export interface InstalledModel {
  path: string;
  name: string;
  size_bytes: number;
  kind: "llm" | "stt";
  in_use: boolean;
  used_by: string | null;
}

/** Progress events are emitted on this channel for every active download. */
export const DOWNLOAD_PROGRESS_EVENT = "model-download-progress";

// ── Command wrappers ──────────────────────────────────────────────────────

export function getHardwareProfile(): Promise<HardwareProfile> {
  return invoke<HardwareProfile>("get_hardware_profile");
}

export function searchHfModels(params: SearchParams): Promise<SearchPage> {
  return invoke<SearchPage>("search_hf_models", { params });
}

export function getHfModel(
  repoId: string,
  hfToken: string | null,
): Promise<HfModelCard> {
  return invoke<HfModelCard>("get_hf_model", { repoId, hfToken });
}

/** Starts a download; resolves to the download id (tracked via the event). */
export function startModelDownload(request: DownloadRequest): Promise<string> {
  return invoke<string>("start_model_download", { request });
}

export function cancelModelDownload(downloadId: string): Promise<void> {
  return invoke("cancel_model_download", { downloadId });
}

export function listInstalledModels(): Promise<InstalledModel[]> {
  return invoke<InstalledModel[]>("list_installed_models");
}

export function deleteInstalledModel(path: string): Promise<void> {
  return invoke("delete_installed_model", { path });
}

/** Subscribes to download progress; returns the unlisten handle. */
export function listenDownloadProgress(
  handler: (progress: DownloadProgress) => void,
): Promise<UnlistenFn> {
  return listen<DownloadProgress>(DOWNLOAD_PROGRESS_EVENT, (event) =>
    handler(event.payload),
  );
}
