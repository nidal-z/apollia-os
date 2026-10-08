/**
 * Models IPC - typed wrappers over the Tauri commands and events that back the
 * AI screen and its discovery window (hardware profile, bounded search, the
 * card of a repository, variant downloads, the installed models and the
 * catalogue of model cards).
 *
 * Every command those surfaces invoke lives here so the Svelte components stay
 * free of raw `invoke(...)` string keys. The shapes mirror the Rust command
 * contract 1:1; keep them in sync when the backend changes.
 */
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { ModelCapabilities } from "$lib/types";

// ── Hardware profile ──────────────────────────────────────────────────────

export interface AcceleratorProfile {
  kind: "none" | "apple_silicon" | "cuda" | "generic";
  chip?: string;
  generation?: number;
  vram_gb?: number;
  device_name?: string;
  compute_capability?: [number, number];
}

/** One graphics device of the machine, as the engine lists it. */
export interface GpuDevice {
  /** The identifier the engine reads (`CUDA0`); `null` when the engine could not be asked. */
  id: string | null;
  name: string;
  /** The interface the engine drives it through (`CUDA`, `Vulkan`, `Metal`). */
  api: string;
  vram_gb: number;
  /** Whether the engine may load onto it. */
  enabled: boolean;
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
  /** The graphics devices, one by one; the figures above leave out the ones held back. */
  devices: GpuDevice[];
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

/** Storage format of the weights. */
export type ModelFormat = "gguf" | "mlx";

/** A level of precision, from about four bits per weight to sixteen. */
export type QuantLevel = "q4" | "q5" | "q6" | "q8" | "fp16";

/** One variant of a model, sized and graded for this machine. */
export interface ModelVariant {
  repo_id: string;
  format: ModelFormat;
  level: QuantLevel;
  /** Quantisation as published: `Q4_K_M`, `BF16`, `4bit`. */
  quantization: string;
  /** The file a download names: the first weights file of a GGUF. */
  origin_file: string;
  files: string[];
  /** Bytes on disk once downloaded, every file of the variant included. */
  size_bytes: number;
  weights_bytes: number;
  sha256?: string | null;
  /** Estimated footprint at the default context, in gibibytes. */
  memory_gb?: number | null;
  compatibility?: Compatibility;
  fully_offloaded?: boolean;
  /** The variant is already in the model index. */
  downloaded?: boolean;
  /** The one variant recommended for this machine. */
  recommended?: boolean;
}

/** What a model can do, as its chat template and files say. */
export interface HubCapabilities {
  tools: boolean;
  reasoning: boolean;
  vision: boolean;
}

export interface HfModelCard {
  repo_id: string;
  author: string | null;
  publisher_kind: PublisherKind;
  downloads: number;
  likes: number;
  /** When the repository last changed, RFC 3339. */
  last_modified?: string | null;
  license: string | null;
  tags: string[];
  gated: boolean;
  total_size_bytes: number | null;
  gguf_files: HfFile[];
  generation_config: GenerationConfig | null;
  compatibility_issue: CompatIssue;
  model_type: string | null;
  capabilities?: HubCapabilities;
  /** The short list of variants; empty on a search result. */
  variants?: ModelVariant[];
}

/** A text Apollia writes in both languages. */
export interface LocalizedText {
  fr: string;
  en: string;
}

/** The card Apollia builds for one search result, from facts only. */
export interface GeneratedCard {
  repo_id: string;
  source_url: string;
  owner: string;
  publisher_kind: PublisherKind;
  /** The model the repository carries, when it declares one. */
  origin_repo: string | null;
  origin_card_id: string | null;
  format: ModelFormat;
  description: LocalizedText;
  parameters: number | null;
  architecture: string | null;
  context_length: number | null;
  capabilities: HubCapabilities;
  license: string | null;
  variants: ModelVariant[];
  size_bytes: number | null;
  last_modified: string | null;
}

/** One search result: what the Hub says, and the card generated for it. */
export interface FoundModel extends HfModelCard {
  card: GeneratedCard;
  /** The repository belongs to the Apollia selection. */
  in_selection: boolean;
}

/** The order of a search. */
export type SearchSort = "best_match" | "likes" | "downloads" | "updated";

export interface SearchParams {
  /** Words of a model name; the search only asks the recognised publishers. */
  query: string;
  format: ModelFormat;
  sort: SearchSort;
  limit?: number;
  /** Keep only the results one of whose variants is recommended here. */
  fits_only: boolean;
}

export interface SearchPage {
  models: FoundModel[];
  format: ModelFormat;
  sort: SearchSort;
  /** What a model can occupy on this machine, GB to the tenth. */
  capacity_gb: number | null;
  /** The organisations that could not be reached. */
  unreachable: string[];
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
  /** Absent or null, the shell uses the token stored by the General settings. */
  hf_token?: string | null;
  /** Destination directory. Defaults to the models folder when absent. */
  dest_dir?: string | null;
}

// ── Installed models ──────────────────────────────────────────────────────

/** What an installed model is for, as the model index files it. */
export type ModelCategory = "language" | "embeddings" | "dictation" | "draft";

export interface InstalledModel {
  /** Identifier in the model index, the name an agent uses. */
  id: string;
  path: string;
  name: string;
  size_bytes: number;
  kind: "llm" | "stt";
  /** Storage format of the weights. */
  format: "gguf" | "mlx";
  /** Category in force, the developer override included. */
  category: ModelCategory;
  /** Parameter count read from the header, for example `"8B"`. */
  parameters: string | null;
  /** Last modification of the weights, RFC 3339. */
  modified_at: string | null;
  /** When the model was pinned to the top of its category, RFC 3339. */
  pinned_at: string | null;
  /** False when the weights matched no published or verified digest. */
  origin_verified: boolean;
  in_use: boolean;
  used_by: string | null;
  /** What the model can do; null until the index is rebuilt by this build. */
  capabilities: ModelCapabilities | null;
  /** The page the model was downloaded from; null for an imported model. */
  source_url: string | null;
  /** The registry entry of an embeddings file, the key search names. */
  embedding_entry: string | null;
  /** Whether this is the model the machine chats with by default. */
  default_chat: boolean;
  /** Whether this is the embeddings model search uses. */
  default_search: boolean;
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

/** The card of one repository; the shell adds the stored token itself. */
export function getHfModel(repoId: string): Promise<HfModelCard> {
  return invoke<HfModelCard>("get_hf_model", { repoId });
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

/**
 * The picture of each organisation of `orgs`, as a `data:` address keyed by
 * the name asked. Read at its source once, then from the copy the machine
 * keeps; an organisation without a picture is absent from the answer.
 */
export function getPublisherAvatars(orgs: string[]): Promise<Record<string, string>> {
  return invoke<Record<string, string>>("get_publisher_avatars", { orgs });
}

/** The README of a repository, as `GET /api/v1/llm/registry/readme` answers. */
export interface ModelReadme {
  /** The repository, `owner/name`. */
  repo: string;
  /** Its owner. */
  author: string;
  /** The licence its front matter declares, when it declares one. */
  license: string | null;
  /** The page of the repository. */
  source_url: string;
  /** The text, as its author wrote it. */
  text: string;
}

/**
 * Reads the README of `repo` at its source. Nothing of it is kept by the
 * shell: the caller holds the text for as long as it shows it. Rejects with
 * the message of the route when the network is cut or the repository has none.
 */
export function getModelReadme(repo: string): Promise<ModelReadme> {
  return invoke<ModelReadme>("get_model_readme", { repo });
}

/** One dictation file of the registry, with its verdict for this machine. */
export interface DictationModel {
  id: string;
  label: string;
  publisher: string;
  /** ISO date the upstream repository was published. */
  released: string;
  license: string;
  /** HuggingFace repository carrying the file, `owner/name`. */
  repo: string;
  file: string;
  quantization: string;
  size_bytes: number;
  sha256: string;
  /** Identifier of the dictation card that describes the model. */
  card: string;
  /** Its footprint, the file plus a fixed margin, in gibibytes. */
  footprint_gb: number;
  /** Whether that footprint fits this machine on its own. */
  fits: boolean;
}

/** The dictation models and the one for this machine, as `GET /api/v1/llm/dictation` answers. */
export interface DictationModels {
  models: DictationModel[];
  /** Identifier of the recommended entry, `null` when the registry has none. */
  recommended: string | null;
}

/** The dictation models of the registry. Answered without the network. */
export function getDictationModels(): Promise<DictationModels> {
  return invoke<DictationModels>("get_dictation_models");
}

/** Subscribes to download progress; returns the unlisten handle. */
export function listenDownloadProgress(
  handler: (progress: DownloadProgress) => void,
): Promise<UnlistenFn> {
  return listen<DownloadProgress>(DOWNLOAD_PROGRESS_EVENT, (event) =>
    handler(event.payload),
  );
}
