/**
 * The models onboarding offers, and the rules that pick among them.
 *
 * Both lists are filtered by the RAM the probe reported, so a machine is never
 * offered weights it cannot hold. A probe that has not answered yet offers
 * everything rather than nothing: the operator can still read the sizes.
 */
import type { SystemInfo } from "$lib/ipc/models";

// ─── Types ────────────────────────────────────────────────────────────────

export interface CuratedLlmModel {
  name: string;
  filename: string;
  url: string;
  size_label: string;
  ram_required: number;
}

export interface CuratedSttModel {
  name: string;
  filename: string;
  url: string;
  repo: string;
  size_label: string;
  ram_required: number;
  quality_key: string;
  lang_key: string;
}

// ─── Curated catalogs ─────────────────────────────────────────────────────
// Qwen3 (April 2025): native tool calling via llama.cpp jinja templates.
// Whisper turbo-q5 is the sweet spot: pruned from large-v3, 6× faster.

export const CURATED_LLM_MODELS: CuratedLlmModel[] = [
  {
    name: "Qwen3 4B",
    filename: "Qwen3-4B-Q4_K_M.gguf",
    url: "https://huggingface.co/Qwen/Qwen3-4B-GGUF/resolve/main/Qwen3-4B-Q4_K_M.gguf",
    size_label: "2.5 GB",
    ram_required: 4,
  },
  {
    name: "Qwen3 8B",
    filename: "Qwen3-8B-Q4_K_M.gguf",
    url: "https://huggingface.co/Qwen/Qwen3-8B-GGUF/resolve/main/Qwen3-8B-Q4_K_M.gguf",
    size_label: "4.7 GB",
    ram_required: 8,
  },
  {
    name: "Qwen3 14B",
    filename: "Qwen3-14B-Q4_K_M.gguf",
    url: "https://huggingface.co/Qwen/Qwen3-14B-GGUF/resolve/main/Qwen3-14B-Q4_K_M.gguf",
    size_label: "8.4 GB",
    ram_required: 16,
  },
  {
    name: "Qwen3 30B-A3B",
    filename: "Qwen3-30B-A3B-Q4_K_M.gguf",
    url: "https://huggingface.co/Qwen/Qwen3-30B-A3B-GGUF/resolve/main/Qwen3-30B-A3B-Q4_K_M.gguf",
    size_label: "18.6 GB",
    ram_required: 24,
  },
];

export const CURATED_STT_MODELS: CuratedSttModel[] = [
  {
    name: "Whisper Tiny",
    filename: "ggml-tiny.bin",
    url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.bin",
    repo: "ggerganov/whisper.cpp",
    size_label: "75 MB",
    ram_required: 1,
    quality_key: "onboarding.ai_setup.stt_quality_ultra_fast",
    lang_key: "onboarding.ai_setup.stt_lang_99",
  },
  {
    name: "Whisper Base",
    filename: "ggml-base.bin",
    url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin",
    repo: "ggerganov/whisper.cpp",
    size_label: "142 MB",
    ram_required: 2,
    quality_key: "onboarding.ai_setup.stt_quality_balanced",
    lang_key: "onboarding.ai_setup.stt_lang_99",
  },
  {
    name: "Whisper Large-v3 Turbo Q5",
    filename: "ggml-large-v3-turbo-q5_0.bin",
    url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-q5_0.bin",
    repo: "ggerganov/whisper.cpp",
    size_label: "547 MB",
    ram_required: 4,
    quality_key: "onboarding.ai_setup.stt_quality_high_6x",
    lang_key: "onboarding.ai_setup.stt_lang_99",
  },
  {
    name: "Whisper Large-v3 Q5",
    filename: "ggml-large-v3-q5_0.bin",
    url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-q5_0.bin",
    repo: "ggerganov/whisper.cpp",
    size_label: "1.1 GB",
    ram_required: 8,
    quality_key: "onboarding.ai_setup.stt_quality_max_precision",
    lang_key: "onboarding.ai_setup.stt_lang_99",
  },
  {
    name: "Whisper Large-v3 French",
    filename: "ggml-model-q5_0.bin",
    url: "https://huggingface.co/bofenghuang/whisper-large-v3-french/resolve/main/ggml-model-q5_0.bin",
    repo: "bofenghuang/whisper-large-v3-french",
    size_label: "~1.1 GB",
    ram_required: 8,
    quality_key: "onboarding.ai_setup.stt_quality_french_tuned",
    lang_key: "onboarding.ai_setup.stt_lang_french",
  },
];

/** A model the machine can hold, or every model while the probe is pending. */
interface Fitting {
  ram_required: number;
}

export function modelsFitting<T extends Fitting>(
  catalog: T[],
  sysInfo: SystemInfo | null,
): T[] {
  return catalog.filter((m) => !sysInfo || sysInfo.total_ram_gb >= m.ram_required);
}

/**
 * The largest model the machine can hold, or `catalog[fallbackIndex]` while the
 * probe is pending.
 */
export function largestFitting<T extends Fitting>(
  catalog: T[],
  sysInfo: SystemInfo | null,
  fallbackIndex: number,
): T | null {
  if (!sysInfo) return catalog[fallbackIndex] ?? null;
  const fitting = modelsFitting(catalog, sysInfo);
  return fitting[fitting.length - 1] ?? fitting[0] ?? null;
}
