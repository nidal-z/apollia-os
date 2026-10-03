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
  /** What a model can occupy here, GB to the tenth: card plus usable system memory on a discrete card. */
  total_capacity_gb: number;
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

/** Whether a repository's owner trained the model or converted it. */
export type PublisherKind = "official" | "converter" | null;

export interface HfModelCard {
  repo_id: string;
  author: string | null;
  publisher_kind: PublisherKind;
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
  /** Why the download stopped, in the runtime's words. Set on `failed` only. */
  error?: string;
}

/** Which variant of a repository to download. */
export type VariantSelector =
  | { kind: "gguf"; file: string }
  | { kind: "mlx" };

/**
 * Request to download one variant whole: every part of a split GGUF, its
 * vision projector when the repository has one, or every file of an MLX
 * repository. One download id, one aggregated progress stream.
 */
export interface VariantDownloadRequest {
  repo_id: string;
  selector: VariantSelector;
  hf_token: string | null;
  /** Destination directory. Defaults to `~/.apollia/models` when absent. */
  dest_dir?: string | null;
}

// ── Installed models ──────────────────────────────────────────────────────

export interface InstalledModel {
  path: string;
  name: string;
  size_bytes: number;
  kind: "llm" | "stt";
  /** False when the weights matched no published or verified digest. */
  origin_verified: boolean;
  in_use: boolean;
  used_by: string | null;
}

/** Progress events are emitted on this channel for every active download. */
const DOWNLOAD_PROGRESS_EVENT = "model-download-progress";

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

/** Starts downloading a whole variant; resolves to its single download id. */
export function startVariantDownload(request: VariantDownloadRequest): Promise<string> {
  return invoke<string>("start_variant_download", { request });
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

// ── Model cards ───────────────────────────────────────────────────────────

/** One card of the catalogue, in the language it was asked in (or English). */
export interface ModelCardView {
  id: string;
  kind: "language" | "embeddings" | "dictation";
  label: string;
  publisher: string;
  language: "fr" | "en";
  description: string;
  license: { id: string; source: string };
  source_url: string;
  capabilities: { tools: boolean; reasoning: boolean; vision: boolean; source: string };
  memory: { min_gb: number; source: string };
  defaults: {
    temperature?: number;
    top_p?: number;
    top_k?: number;
    min_p?: number;
    repetition_penalty?: number;
    presence_penalty?: number;
    source: string;
  };
  author_fields: {
    name: string;
    type: "liste" | "booleen";
    values?: unknown[];
    default: unknown;
    explanation: string;
    source: string;
  }[];
  repos: string[];
}

/** The catalogue as it opens: the cards, or an empty list with the reason. */
export interface ModelCatalogue {
  status: "ok" | "offline" | "disabled" | "no_trusted_key";
  origin: "primary" | "mirror" | "cache" | null;
  message: string | null;
  cards: ModelCardView[];
  publishers: { official: { publisher: string; orgs: string[] }[]; converters: string[] };
}

/**
 * Opens the catalogue: reads the signed model cards (main address, mirror,
 * then the copy kept from the last read) in `lang`, keeping a copy so the card
 * of a model downloaded afterwards is written next to it.
 */
export function listModelCards(lang: string): Promise<ModelCatalogue> {
  return invoke<ModelCatalogue>("list_model_cards", { lang });
}

/** Subscribes to download progress; returns the unlisten handle. */
export function listenDownloadProgress(
  handler: (progress: DownloadProgress) => void,
): Promise<UnlistenFn> {
  return listen<DownloadProgress>(DOWNLOAD_PROGRESS_EVENT, (event) =>
    handler(event.payload),
  );
}
