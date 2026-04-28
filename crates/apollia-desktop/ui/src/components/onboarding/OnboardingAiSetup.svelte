<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import {
    Cpu, HardDrive, Mic, Check, ChevronRight, AlertCircle,
    MemoryStick, MonitorCog, Download, X, Search, ArrowLeft, RefreshCw,
  } from "lucide-svelte";
  import { Spinner } from "$lib/components/ui/progress";
  import { t } from "svelte-i18n";
  import { onboardingStore } from "$lib/stores/onboarding";

  // ─── Types ────────────────────────────────────────────────────────────────

  interface SystemInfo {
    total_ram_gb: number;
    available_ram_gb: number;
    os: string;
    arch: string;
    gpu_available: boolean;
  }

  interface GgufModelInfo {
    path: string;
    filename: string;
    size_bytes: number;
    size_human: string;
    recommended: boolean;
  }

  interface WhisperModelInfo {
    path: string;
    filename: string;
    size_bytes: number;
    model_size: string;
    recommended: boolean;
  }

  interface DownloadProgress {
    id: string;
    downloaded_bytes: number;
    total_bytes: number | null;
    speed_bps: number;
    dest_path: string;
    status: "in_progress" | "completed" | "cancelled" | "failed";
  }

  interface HfFile {
    filename: string;
    size_bytes: number;
    size_human: string;
    compatibility: "fits" | "might_fit" | "too_large" | null;
    download_url: string;
  }

  interface HfModelCard {
    repo_id: string;
    gated: boolean;
    gguf_files: HfFile[];
    compatibility_issue: "embedding_model" | "unknown_architecture" | "no_gguf_files" | null;
  }

  interface CuratedLlmModel {
    name: string;
    filename: string;
    url: string;
    size_label: string;
    ram_required: number;
  }

  interface CuratedSttModel {
    name: string;
    filename: string;
    url: string;
    repo: string;
    size_label: string;
    ram_required: number;
    quality_label: string;
    lang: string;
  }

  // ─── Curated models ───────────────────────────────────────────────────────
  // All LLMs: Qwen3 (April 2025) — native tool calling via llama.cpp jinja templates.
  // Filenames verified against Qwen/Qwen3-*-GGUF repos on HuggingFace.
  // All STT from ggerganov/whisper.cpp or bofenghuang — ggml-*.bin pattern.

  const CURATED_LLM_MODELS: CuratedLlmModel[] = [
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

  // large-v3-turbo-q5_0 is the sweet spot: pruned from large-v3, 6× faster, < 1% WER diff.
  // bofenghuang/whisper-large-v3-french: fine-tuned on French, converts to GGML ggml-model-q5_0.bin.
  const CURATED_STT_MODELS: CuratedSttModel[] = [
    {
      name: "Whisper Tiny",
      filename: "ggml-tiny.bin",
      url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.bin",
      repo: "ggerganov/whisper.cpp",
      size_label: "75 MB",
      ram_required: 1,
      quality_label: "Ultra-rapide",
      lang: "99 langues",
    },
    {
      name: "Whisper Base",
      filename: "ggml-base.bin",
      url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin",
      repo: "ggerganov/whisper.cpp",
      size_label: "142 MB",
      ram_required: 2,
      quality_label: "Équilibré",
      lang: "99 langues",
    },
    {
      name: "Whisper Large-v3 Turbo Q5",
      filename: "ggml-large-v3-turbo-q5_0.bin",
      url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-q5_0.bin",
      repo: "ggerganov/whisper.cpp",
      size_label: "547 MB",
      ram_required: 4,
      quality_label: "Haute qualité · 6× rapide",
      lang: "99 langues",
    },
    {
      name: "Whisper Large-v3 Q5",
      filename: "ggml-large-v3-q5_0.bin",
      url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-q5_0.bin",
      repo: "ggerganov/whisper.cpp",
      size_label: "1.1 GB",
      ram_required: 8,
      quality_label: "Précision maximale",
      lang: "99 langues",
    },
    {
      name: "Whisper Large-v3 French",
      filename: "ggml-model-q5_0.bin",
      url: "https://huggingface.co/bofenghuang/whisper-large-v3-french/resolve/main/ggml-model-q5_0.bin",
      repo: "bofenghuang/whisper-large-v3-french",
      size_label: "~1.1 GB",
      ram_required: 8,
      quality_label: "Fine-tuné français",
      lang: "🇫🇷 Français",
    },
  ];

  // ─── State ────────────────────────────────────────────────────────────────

  let visible = $state(false);
  let loading = $state(true);
  let sysInfo = $state<SystemInfo | null>(null);
  let ggufModels = $state<GgufModelInfo[]>([]);
  let whisperModels = $state<WhisperModelInfo[]>([]);

  let selectedGguf = $state<GgufModelInfo | null>(null);
  let llmConfiguring = $state(false);
  let llmSuccess = $state(false);
  let llmError = $state<string | null>(null);

  let sttEnabled = $state(false);
  let selectedWhisper = $state<WhisperModelInfo | null>(null);

  // LLM download
  let llmDownloadId = $state<string | null>(null);
  let llmDownloadProgress = $state<DownloadProgress | null>(null);
  let llmDownloadingModel = $state<CuratedLlmModel | HfFile | null>(null);
  let llmDownloadError = $state<string | null>(null);

  // STT download
  let sttDownloadId = $state<string | null>(null);
  let sttDownloadProgress = $state<DownloadProgress | null>(null);
  let sttDownloadingModel = $state<CuratedSttModel | null>(null);
  let sttDownloadError = $state<string | null>(null);

  // HF search (LLM)
  let showSearch = $state(false);
  let searchQuery = $state("");
  let searchLoading = $state(false);
  let searchResults = $state<HfModelCard[]>([]);
  let searchError = $state<string | null>(null);
  let expandedModel = $state<string | null>(null);
  let expandedDetail = $state<HfModelCard | null>(null);
  let expandLoading = $state(false);

  let advancing = $state(false);
  let advanceError = $state<string | null>(null);

  // ─── Derived ─────────────────────────────────────────────────────────────

  let availableLlmModels = $derived(
    CURATED_LLM_MODELS.filter((m) => !sysInfo || sysInfo.total_ram_gb >= m.ram_required),
  );

  // The single best LLM for this machine: largest fitting model.
  let recommendedLlm = $derived<CuratedLlmModel | null>(() => {
    if (!sysInfo) return CURATED_LLM_MODELS[1] ?? null;
    const fitting = CURATED_LLM_MODELS.filter((m) => sysInfo!.total_ram_gb >= m.ram_required);
    return fitting[fitting.length - 1] ?? fitting[0] ?? null;
  });

  let availableSttModels = $derived(
    CURATED_STT_MODELS.filter((m) => !sysInfo || sysInfo.total_ram_gb >= m.ram_required),
  );

  // The single best STT: large-v3-turbo-q5 if fits, otherwise largest fitting.
  let recommendedStt = $derived<CuratedSttModel | null>(() => {
    if (!sysInfo) return CURATED_STT_MODELS[2] ?? null;
    const turbo = CURATED_STT_MODELS.find((m) => m.filename.includes("turbo"));
    if (turbo && sysInfo!.total_ram_gb >= turbo.ram_required) return turbo;
    const fitting = CURATED_STT_MODELS.filter((m) => sysInfo!.total_ram_gb >= m.ram_required);
    return fitting[fitting.length - 1] ?? fitting[0] ?? null;
  });

  // ─── Effects ─────────────────────────────────────────────────────────────

  $effect(() => {
    requestAnimationFrame(() => { visible = true; });
    void loadData();
  });

  $effect(() => {
    let unlisten: (() => void) | undefined;
    listen<DownloadProgress>("model-download-progress", (event) => {
      const p = event.payload;
      if (p.id === llmDownloadId) {
        llmDownloadProgress = p;
        if (p.status === "completed") {
          llmDownloadId = null;
          llmDownloadingModel = null;
          void loadData();
        } else if (p.status === "cancelled" || p.status === "failed") {
          llmDownloadId = null;
          llmDownloadingModel = null;
          llmDownloadProgress = null;
          if (p.status === "failed") llmDownloadError = "Le téléchargement a échoué.";
        }
      } else if (p.id === sttDownloadId) {
        sttDownloadProgress = p;
        if (p.status === "completed") {
          sttDownloadId = null;
          sttDownloadingModel = null;
          void rescanStt();
        } else if (p.status === "cancelled" || p.status === "failed") {
          sttDownloadId = null;
          sttDownloadingModel = null;
          sttDownloadProgress = null;
          if (p.status === "failed") sttDownloadError = "Le téléchargement a échoué.";
        }
      }
    }).then((fn) => { unlisten = fn; });
    return () => { unlisten?.(); };
  });

  // ─── Data loading ────────────────────────────────────────────────────────

  async function loadData(): Promise<void> {
    try {
      const [sys, gguf, whisper] = await Promise.all([
        invoke<SystemInfo>("get_ai_setup_info"),
        invoke<GgufModelInfo[]>("scan_for_gguf_models"),
        invoke<WhisperModelInfo[]>("scan_for_whisper_models"),
      ]);
      sysInfo = sys;
      ggufModels = gguf;
      whisperModels = whisper;
      if (whisper.length > 0) {
        selectedWhisper = whisper[0];
        sttEnabled = whisper[0].recommended;
      }
    } catch { /* leave empty */ } finally {
      loading = false;
    }
  }

  async function rescanStt(): Promise<void> {
    try {
      const whisper = await invoke<WhisperModelInfo[]>("scan_for_whisper_models");
      whisperModels = whisper;
      if (whisper.length > 0) {
        selectedWhisper = whisper[0];
        sttEnabled = whisper[0].recommended;
      }
    } catch { /* leave empty */ }
  }

  // ─── LLM local model selection ───────────────────────────────────────────

  async function selectGgufModel(model: GgufModelInfo): Promise<void> {
    if (llmConfiguring || llmSuccess) return;
    selectedGguf = model;
    llmConfiguring = true;
    llmError = null;
    try {
      await invoke("setup_local_llm", { ggufPath: model.path });
      await invoke("reload_llm");
      llmSuccess = true;
    } catch (err: unknown) {
      llmError = err instanceof Error ? err.message : String(err);
      llmConfiguring = false;
    }
  }

  // ─── LLM download (curated) ──────────────────────────────────────────────

  async function downloadLlmModel(model: CuratedLlmModel): Promise<void> {
    if (llmDownloadId) return;
    llmDownloadError = null;
    llmDownloadProgress = null;
    llmDownloadingModel = model;
    try {
      const id = await invoke<string>("start_model_download", {
        request: { url: model.url, filename: model.filename },
      });
      llmDownloadId = id;
    } catch (err: unknown) {
      llmDownloadError = err instanceof Error ? err.message : String(err);
      llmDownloadingModel = null;
    }
  }

  async function cancelLlmDownload(): Promise<void> {
    if (!llmDownloadId) return;
    try { await invoke("cancel_model_download", { downloadId: llmDownloadId }); } catch { /* ignore */ }
  }

  // ─── LLM HuggingFace search ──────────────────────────────────────────────

  async function searchHf(): Promise<void> {
    if (!searchQuery.trim() || searchLoading) return;
    searchLoading = true;
    searchError = null;
    searchResults = [];
    expandedModel = null;
    expandedDetail = null;
    try {
      const data = await invoke<{ models: HfModelCard[]; next_cursor: string | null }>(
        "search_hf_models",
        { params: { query: searchQuery.trim(), limit: 8 } },
      );
      searchResults = data.models;
    } catch (err: unknown) {
      searchError = err instanceof Error ? err.message : String(err);
    } finally {
      searchLoading = false;
    }
  }

  async function expandSearchModel(repoId: string): Promise<void> {
    if (expandedModel === repoId) { expandedModel = null; expandedDetail = null; return; }
    expandedModel = repoId;
    expandedDetail = null;
    expandLoading = true;
    try {
      expandedDetail = await invoke<HfModelCard>("get_hf_model", { repoId });
    } catch { /* show what we have */ } finally {
      expandLoading = false;
    }
  }

  async function downloadHfFile(file: HfFile): Promise<void> {
    if (llmDownloadId) return;
    llmDownloadError = null;
    llmDownloadProgress = null;
    llmDownloadingModel = file;
    try {
      const id = await invoke<string>("start_model_download", {
        request: { url: file.download_url, filename: file.filename },
      });
      llmDownloadId = id;
      showSearch = false;
    } catch (err: unknown) {
      llmDownloadError = err instanceof Error ? err.message : String(err);
      llmDownloadingModel = null;
    }
  }

  // ─── STT download ────────────────────────────────────────────────────────

  async function downloadSttModel(model: CuratedSttModel): Promise<void> {
    if (sttDownloadId) return;
    sttDownloadError = null;
    sttDownloadProgress = null;
    sttDownloadingModel = model;
    try {
      const id = await invoke<string>("start_model_download", {
        request: { url: model.url, filename: model.filename },
      });
      sttDownloadId = id;
    } catch (err: unknown) {
      sttDownloadError = err instanceof Error ? err.message : String(err);
      sttDownloadingModel = null;
    }
  }

  async function cancelSttDownload(): Promise<void> {
    if (!sttDownloadId) return;
    try { await invoke("cancel_model_download", { downloadId: sttDownloadId }); } catch { /* ignore */ }
  }

  // ─── Navigation ──────────────────────────────────────────────────────────

  async function handleContinue(): Promise<void> {
    if (advancing) return;
    advancing = true;
    advanceError = null;
    try {
      if (sttEnabled && selectedWhisper) {
        await invoke("setup_whisper_model", { modelPath: selectedWhisper.path });
      }
      await onboardingStore.advancePhase("acquaintance");
    } catch (err: unknown) {
      advanceError = err instanceof Error ? err.message : String(err);
      advancing = false;
    }
  }

  // ─── Helpers ─────────────────────────────────────────────────────────────

  function ramLabel(gb: number): string {
    return gb >= 1 ? `${Math.round(gb)} GB` : `${Math.round(gb * 1024)} MB`;
  }

  function osLabel(os: string): string {
    if (os === "macos") return "macOS";
    if (os === "linux") return "Linux";
    if (os === "windows") return "Windows";
    return os;
  }

  function dlBytes(p: DownloadProgress): string {
    const dl = (p.downloaded_bytes / 1e9).toFixed(2);
    if (!p.total_bytes) return `${dl} GB`;
    const total = (p.total_bytes / 1e9).toFixed(2);
    const pct = Math.round((p.downloaded_bytes / p.total_bytes) * 100);
    return `${dl} / ${total} GB · ${pct}%`;
  }

  function dlSpeed(bps: number): string {
    return bps >= 1e6 ? `${(bps / 1e6).toFixed(1)} MB/s` : `${Math.round(bps / 1000)} KB/s`;
  }

  function dlPct(p: DownloadProgress): number {
    if (!p.total_bytes) return 0;
    return Math.min(100, Math.round((p.downloaded_bytes / p.total_bytes) * 100));
  }

  function hfFileLabel(f: HfFile): string {
    if (f.compatibility === "fits") return "fits";
    if (f.compatibility === "might_fit") return "might fit";
    if (f.compatibility === "too_large") return "too large";
    return "";
  }
</script>

<div class="ai-setup-screen" class:visible data-testid="onboarding-ai-setup">
  <div class="ai-setup-content">

    <!-- Header -->
    <header class="ai-setup-header">
      <img src="/logo.svg" alt="Apollia OS" class="ai-setup-logo" />
      <h1 class="ai-setup-title">{$t("onboarding_v2.ai_setup.title")}</h1>
      <p class="ai-setup-subtitle">{$t("onboarding_v2.ai_setup.subtitle")}</p>
    </header>

    <!-- System info bar -->
    {#if sysInfo}
      <div class="sys-info-bar" data-testid="system-info-bar">
        <span class="sys-chip"><MemoryStick size={12} strokeWidth={2} />{ramLabel(sysInfo.total_ram_gb)} RAM</span>
        <span class="sys-chip"><MonitorCog size={12} strokeWidth={2} />{osLabel(sysInfo.os)} · {sysInfo.arch}</span>
        {#if sysInfo.gpu_available}<span class="sys-chip sys-chip-gpu">GPU</span>{/if}
      </div>
    {/if}

    {#if loading}
      <div class="scan-loading" data-testid="scan-loading">
        <Spinner size={20} />
        <span>{$t("onboarding_v2.ai_setup.scanning")}</span>
      </div>
    {:else}

      <!-- ── LLM section ──────────────────────────────────────────────── -->
      <section class="setup-section" data-testid="llm-section">
        <div class="section-header">
          <HardDrive size={15} strokeWidth={2} class="text-primary" />
          <span class="section-title">{$t("onboarding_v2.ai_setup.llm_section.title")}</span>
          {#if llmSuccess}
            <span class="section-badge-ok">
              <Check size={11} strokeWidth={2.5} /> {$t("onboarding_v2.ai_setup.llm_section.configured")}
            </span>
          {/if}
        </div>

        {#if ggufModels.length === 0 && !llmSuccess}

          <!-- Manual placement hint -->
          <p class="empty-hint" data-testid="llm-empty-hint">
            {$t("onboarding_v2.ai_setup.llm_section.no_models")}
            <code>~/.apollia/models/</code> ou <code>~/Downloads/</code>, puis
            <button class="inline-link" onclick={loadData}>{$t("onboarding_v2.ai_setup.llm_section.rescan")}</button>.
          </p>

          <!-- Download in progress -->
          {#if llmDownloadId}
            <div class="download-block" data-testid="llm-download-progress">
              <div class="dl-header">
                <span class="dl-filename">
                  {"filename" in (llmDownloadingModel ?? {}) ? (llmDownloadingModel as CuratedLlmModel | HfFile).filename : "…"}
                </span>
                <button class="btn-cancel-dl" onclick={cancelLlmDownload} aria-label="Annuler">
                  <X size={13} strokeWidth={2} />
                </button>
              </div>
              <div class="progress-track">
                {#if llmDownloadProgress}
                  <div class="progress-fill" style="width:{dlPct(llmDownloadProgress)}%"></div>
                {:else}
                  <div class="progress-fill indeterminate"></div>
                {/if}
              </div>
              {#if llmDownloadProgress}
                <div class="dl-meta"><span>{dlBytes(llmDownloadProgress)}</span><span>{dlSpeed(llmDownloadProgress.speed_bps)}</span></div>
              {/if}
            </div>

          {:else if showSearch}
            <!-- ── HuggingFace search mode ── -->
            <div class="search-bar">
              <button class="btn-back" onclick={() => { showSearch = false; searchResults = []; }}>
                <ArrowLeft size={13} strokeWidth={2} />
              </button>
              <input
                class="search-input"
                type="text"
                placeholder="Rechercher un modèle…"
                bind:value={searchQuery}
                onkeydown={(e) => e.key === "Enter" && searchHf()}
              />
              <button class="btn-search" onclick={searchHf} disabled={searchLoading}>
                {#if searchLoading}<Spinner size={13} />{:else}<Search size={13} strokeWidth={2} />{/if}
              </button>
            </div>

            {#if searchError}
              <p class="inline-error" role="alert"><AlertCircle size={13} />{searchError}</p>
            {/if}

            {#if searchResults.length > 0}
              <ul class="model-list" data-testid="search-results">
                {#each searchResults as model (model.repo_id)}
                  {@const isExpanded = expandedModel === model.repo_id}
                  {@const detail = isExpanded ? expandedDetail : null}
                  <li class="search-result-item" class:is-expanded={isExpanded}>
                    <button
                      class="search-result-header"
                      onclick={() => expandSearchModel(model.repo_id)}
                      disabled={model.compatibility_issue === "embedding_model" || model.compatibility_issue === "no_gguf_files"}
                    >
                      <div class="model-icon">
                        <Cpu size={13} strokeWidth={1.75} />
                      </div>
                      <span class="model-name">{model.repo_id}</span>
                      {#if model.gated}
                        <span class="badge-gated">gated</span>
                      {/if}
                      <ChevronRight size={13} class="text-muted-foreground/50 transition-transform {isExpanded ? 'rotate-90' : ''}" />
                    </button>

                    {#if isExpanded}
                      <div class="search-result-files">
                        {#if expandLoading && !detail}
                          <div class="files-loading"><Spinner size={13} /></div>
                        {:else}
                          {@const files = (detail ?? model).gguf_files.slice(0, 6)}
                          {#each files as file (file.filename)}
                            <button
                              class="file-row"
                              class:compat-fits={file.compatibility === "fits"}
                              class:compat-large={file.compatibility === "too_large"}
                              onclick={() => downloadHfFile(file)}
                              disabled={file.compatibility === "too_large"}
                            >
                              <Download size={11} strokeWidth={2} />
                              <span class="file-name">{file.filename}</span>
                              <span class="file-size">{file.size_human}</span>
                              {#if file.compatibility}
                                <span class="compat-chip compat-chip-{file.compatibility}">{hfFileLabel(file)}</span>
                              {/if}
                            </button>
                          {/each}
                        {/if}
                      </div>
                    {/if}
                  </li>
                {/each}
              </ul>
            {:else if !searchLoading}
              <p class="empty-hint" style="text-align:center">
                {searchQuery ? "Aucun résultat." : "Tapez un nom de modèle pour commencer."}
              </p>
            {/if}

          {:else}
            <!-- ── Curated models ── -->
            <div class="curated-divider"><span>ou télécharger un modèle recommandé</span></div>
            <ul class="model-list" data-testid="curated-llm-list">
              {#each availableLlmModels as model (model.filename)}
                <li>
                  <button class="model-row" onclick={() => downloadLlmModel(model)} data-testid="curated-llm-row">
                    <div class="model-icon"><Download size={13} strokeWidth={1.75} /></div>
                    <div class="model-info">
                      <span class="model-name">{model.name}</span>
                      <span class="model-meta">{model.size_label}</span>
                    </div>
                    {#if model.filename === recommendedLlm?.filename}
                      <span class="badge-recommended">{$t("onboarding_v2.ai_setup.llm_section.recommended")}</span>
                    {/if}
                    <ChevronRight size={14} class="text-muted-foreground/50" />
                  </button>
                </li>
              {/each}
            </ul>
            <button class="btn-search-hf" onclick={() => { showSearch = true; searchResults = []; }}>
              <Search size={12} strokeWidth={2} />
              Rechercher sur HuggingFace
            </button>
          {/if}

          {#if llmDownloadError}
            <p class="inline-error" role="alert" data-testid="llm-download-error">
              <AlertCircle size={13} />{llmDownloadError}
            </p>
          {/if}

        {:else if llmSuccess}
          <div class="success-row" data-testid="llm-success">
            <div class="success-icon-sm"><Check size={14} strokeWidth={2.5} class="text-white" /></div>
            <span class="success-filename">{selectedGguf?.filename}</span>
          </div>
        {:else}
          <ul class="model-list" data-testid="llm-model-list">
            {#each ggufModels as model (model.path)}
              <li>
                <button
                  class="model-row"
                  class:is-selected={selectedGguf?.path === model.path && llmConfiguring}
                  onclick={() => selectGgufModel(model)}
                  disabled={llmConfiguring || llmSuccess}
                  data-testid="llm-model-row"
                >
                  <div class="model-icon">
                    {#if selectedGguf?.path === model.path && llmConfiguring}
                      <Spinner size={14} />
                    {:else}
                      <Cpu size={14} strokeWidth={1.75} />
                    {/if}
                  </div>
                  <div class="model-info">
                    <span class="model-name">{model.filename}</span>
                    <span class="model-meta">{model.size_human}</span>
                  </div>
                  {#if model.recommended}
                    <span class="badge-recommended">{$t("onboarding_v2.ai_setup.llm_section.recommended")}</span>
                  {/if}
                  <ChevronRight size={14} class="text-muted-foreground/50" />
                </button>
              </li>
            {/each}
          </ul>
          {#if llmError}
            <p class="inline-error" role="alert" data-testid="llm-error"><AlertCircle size={13} />{llmError}</p>
          {/if}
        {/if}
      </section>

      <!-- ── STT section ──────────────────────────────────────────────── -->
      <section class="setup-section" data-testid="stt-section">
        <div class="section-header">
          <Mic size={15} strokeWidth={2} class="text-secondary" />
          <span class="section-title">{$t("onboarding_v2.ai_setup.stt_section.title")}</span>
          <button class="btn-rescan" onclick={rescanStt} aria-label="Re-scanner" title="Re-scanner">
            <RefreshCw size={12} strokeWidth={2} />
          </button>
          <label class="toggle-label">
            <input
              type="checkbox"
              class="toggle-input"
              bind:checked={sttEnabled}
              disabled={whisperModels.length === 0}
              data-testid="stt-toggle"
            />
            <span class="toggle-track" class:on={sttEnabled}></span>
          </label>
        </div>

        {#if whisperModels.length === 0}
          <p class="empty-hint" data-testid="stt-empty-hint">
            {$t("onboarding_v2.ai_setup.llm_section.no_whisper")}
          </p>

          {#if sttDownloadId}
            <div class="download-block" data-testid="stt-download-progress">
              <div class="dl-header">
                <span class="dl-filename">{sttDownloadingModel?.filename ?? "…"}</span>
                <button class="btn-cancel-dl" onclick={cancelSttDownload} aria-label="Annuler">
                  <X size={13} strokeWidth={2} />
                </button>
              </div>
              <div class="progress-track">
                {#if sttDownloadProgress}
                  <div class="progress-fill" style="width:{dlPct(sttDownloadProgress)}%"></div>
                {:else}
                  <div class="progress-fill indeterminate"></div>
                {/if}
              </div>
              {#if sttDownloadProgress}
                <div class="dl-meta"><span>{dlBytes(sttDownloadProgress)}</span><span>{dlSpeed(sttDownloadProgress.speed_bps)}</span></div>
              {/if}
            </div>
          {:else}
            <div class="curated-divider"><span>télécharger Whisper</span></div>
            <ul class="model-list" data-testid="curated-stt-list">
              {#each availableSttModels as model (model.filename)}
                <li>
                  <button class="model-row" onclick={() => downloadSttModel(model)} data-testid="curated-stt-row">
                    <div class="model-icon model-icon-stt"><Download size={13} strokeWidth={1.75} /></div>
                    <div class="model-info">
                      <span class="model-name">{model.name}</span>
                      <span class="model-meta">{model.size_label} · {model.quality_label} · <span class="model-lang">{model.lang}</span></span>
                    </div>
                    {#if model.filename === recommendedStt?.filename}
                      <span class="badge-recommended badge-recommended-stt">{$t("onboarding_v2.ai_setup.stt_section.recommended")}</span>
                    {/if}
                    <ChevronRight size={14} class="text-muted-foreground/50" />
                  </button>
                </li>
              {/each}
            </ul>
          {/if}

          {#if sttDownloadError}
            <p class="inline-error" role="alert" data-testid="stt-download-error">
              <AlertCircle size={13} />{sttDownloadError}
            </p>
          {/if}

        {:else if sttEnabled}
          <ul class="model-list" data-testid="whisper-model-list">
            {#each whisperModels as model (model.path)}
              <li>
                <button
                  class="model-row"
                  class:is-selected={selectedWhisper?.path === model.path}
                  onclick={() => { selectedWhisper = model; }}
                  data-testid="whisper-model-row"
                >
                  <div class="model-icon model-icon-stt"><Mic size={13} strokeWidth={1.75} /></div>
                  <div class="model-info">
                    <span class="model-name">{model.filename}</span>
                    <span class="model-meta capitalize">{model.model_size}</span>
                  </div>
                  {#if model.recommended}
                    <span class="badge-recommended badge-recommended-stt">{$t("onboarding_v2.ai_setup.stt_section.recommended")}</span>
                  {/if}
                  {#if selectedWhisper?.path === model.path}
                    <Check size={14} strokeWidth={2.5} class="text-secondary" />
                  {:else}
                    <ChevronRight size={14} class="text-muted-foreground/50" />
                  {/if}
                </button>
              </li>
            {/each}
          </ul>
        {/if}
      </section>
    {/if}

    {#if advanceError}
      <p class="inline-error" role="alert" data-testid="advance-error">
        <AlertCircle size={13} />{advanceError}
      </p>
    {/if}

    <!-- Footer -->
    <footer class="ai-setup-footer">
      <button
        class="btn-continue"
        onclick={handleContinue}
        disabled={advancing || loading}
        data-testid="ai-setup-continue"
      >
        {#if advancing}
          <Spinner size={15} />
          {$t("onboarding_v2.ai_setup.loading")}
        {:else}
          {$t("onboarding_v2.ai_setup.continue")}
          <ChevronRight size={15} strokeWidth={2} />
        {/if}
      </button>
      <button
        class="btn-skip"
        onclick={() => onboardingStore.advancePhase("acquaintance")}
        disabled={advancing}
        data-testid="ai-setup-skip"
      >
        {$t("onboarding_v2.ai_setup.configure_later")}
      </button>
    </footer>

  </div>
</div>

<style>
  .ai-setup-screen {
    position: fixed;
    inset: 0;
    z-index: 50;
    display: flex;
    align-items: flex-start;
    justify-content: center;
    background: hsl(var(--background));
    overflow-y: auto;
    opacity: 0;
    transition: opacity 300ms ease-in;
  }
  .ai-setup-screen.visible { opacity: 1; }

  .ai-setup-content {
    width: 100%;
    max-width: 30rem;
    padding: 2.5rem 1.25rem 2rem;
    display: flex;
    flex-direction: column;
    gap: 1.25rem;
  }

  /* Header */
  .ai-setup-header {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.5rem;
    text-align: center;
  }
  .ai-setup-logo {
    width: 4rem;
    height: 4rem;
    object-fit: contain;
    margin-bottom: 0.25rem;
  }
  .ai-setup-title {
    font-size: 1.5rem;
    font-weight: 700;
    color: hsl(var(--foreground));
    margin: 0;
    letter-spacing: -0.02em;
  }
  .ai-setup-subtitle {
    font-size: 0.8125rem;
    color: hsl(var(--muted-foreground));
    margin: 0;
    line-height: 1.55;
  }

  /* Sys chips */
  .sys-info-bar { display: flex; gap: 0.5rem; flex-wrap: wrap; justify-content: center; }
  .sys-chip {
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
    font-size: 0.6875rem;
    font-weight: 500;
    color: hsl(var(--muted-foreground));
    background: hsl(var(--muted-foreground) / 0.08);
    padding: 0.25rem 0.625rem;
    border-radius: 99px;
  }
  .sys-chip-gpu { color: hsl(var(--secondary)); background: hsl(var(--secondary) / 0.1); }

  /* Scan loading */
  .scan-loading {
    display: flex; align-items: center; justify-content: center;
    gap: 0.625rem; padding: 1.5rem 0;
    font-size: 0.875rem; color: hsl(var(--muted-foreground));
  }

  /* Section */
  .setup-section {
    background: hsl(var(--card) / 0.72);
    border: 1px solid hsl(var(--primary) / 0.07);
    border-radius: 1rem;
    padding: 1rem;
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
    box-shadow: var(--shadow-elev-1);
  }
  .section-header { display: flex; align-items: center; gap: 0.5rem; }
  .section-title { flex: 1; font-size: 0.875rem; font-weight: 600; color: hsl(var(--foreground)); }
  .section-badge-ok {
    display: inline-flex; align-items: center; gap: 0.25rem;
    font-size: 0.6875rem; font-weight: 600;
    color: hsl(var(--success)); background: hsl(var(--success) / 0.1);
    padding: 0.2rem 0.5rem; border-radius: 99px;
  }

  /* Model list */
  .model-list { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 0.375rem; }
  .model-row {
    display: flex; align-items: center; gap: 0.625rem;
    width: 100%; padding: 0.625rem 0.75rem;
    border-radius: 0.625rem; border: 1px solid hsl(var(--primary) / 0.07);
    background: hsl(var(--card) / 0.6);
    cursor: pointer; text-align: left;
    transition: background 120ms ease, border-color 120ms ease;
  }
  .model-row:hover:not(:disabled) { background: hsl(var(--primary) / 0.04); border-color: hsl(var(--primary) / 0.15); }
  .model-row:disabled { opacity: 0.55; cursor: not-allowed; }
  .model-row.is-selected { border-color: hsl(var(--primary) / 0.3); background: hsl(var(--primary) / 0.04); }

  .model-icon {
    width: 1.75rem; height: 1.75rem; border-radius: 0.4rem;
    background: hsl(var(--primary) / 0.08); display: flex; align-items: center; justify-content: center;
    color: hsl(var(--primary)); flex-shrink: 0;
  }
  .model-icon-stt { background: hsl(var(--secondary) / 0.1); color: hsl(var(--secondary)); }

  .model-info { flex: 1; display: flex; flex-direction: column; gap: 0.1rem; min-width: 0; }
  .model-name { font-size: 0.8125rem; font-weight: 500; color: hsl(var(--foreground)); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .model-meta { font-size: 0.6875rem; color: hsl(var(--muted-foreground) / 0.7); }
  .model-lang { font-weight: 500; }
  .capitalize { text-transform: capitalize; }

  .badge-recommended {
    font-size: 0.6rem; font-weight: 700; letter-spacing: 0.04em; text-transform: uppercase;
    color: hsl(var(--primary)); background: hsl(var(--primary) / 0.08);
    padding: 0.2rem 0.45rem; border-radius: 99px; flex-shrink: 0;
  }
  .badge-recommended-stt { color: hsl(var(--secondary)); background: hsl(var(--secondary) / 0.1); }

  /* Success */
  .success-row { display: flex; align-items: center; gap: 0.625rem; }
  .success-icon-sm {
    width: 1.5rem; height: 1.5rem; border-radius: 50%;
    background: linear-gradient(135deg, hsl(var(--success)), hsl(var(--success) / 0.8));
    display: flex; align-items: center; justify-content: center; flex-shrink: 0;
  }
  .success-filename { font-size: 0.8125rem; font-weight: 500; color: hsl(var(--success)); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }

  /* Rescan */
  .btn-rescan {
    display: flex; align-items: center; justify-content: center;
    width: 1.5rem; height: 1.5rem; border-radius: 0.375rem; border: 1px solid hsl(var(--border));
    background: transparent; color: hsl(var(--muted-foreground));
    cursor: pointer; flex-shrink: 0; transition: background 120ms ease, color 120ms ease;
  }
  .btn-rescan:hover { background: hsl(var(--secondary) / 0.08); color: hsl(var(--secondary)); }

  /* Toggle */
  .toggle-label { display: flex; align-items: center; cursor: pointer; flex-shrink: 0; }
  .toggle-input { position: absolute; opacity: 0; width: 0; height: 0; }
  .toggle-track {
    width: 2.25rem; height: 1.25rem; border-radius: 99px;
    background: hsl(var(--muted)); position: relative; transition: background 150ms ease;
  }
  .toggle-track::after {
    content: ""; position: absolute; top: 0.1875rem; left: 0.1875rem;
    width: 0.875rem; height: 0.875rem; border-radius: 50%; background: white;
    transition: transform 150ms ease; box-shadow: var(--shadow-elev-1);
  }
  .toggle-track.on { background: hsl(var(--secondary)); }
  .toggle-track.on::after { transform: translateX(1rem); }

  /* Empty hints */
  .empty-hint { font-size: 0.8125rem; color: hsl(var(--muted-foreground)); margin: 0; line-height: 1.55; }
  .empty-hint code {
    font-family: monospace; font-size: 0.8em;
    background: hsl(var(--muted-foreground) / 0.1); padding: 0.1em 0.3em;
    border-radius: 0.25rem; color: hsl(var(--foreground) / 0.8);
  }
  .inline-link {
    background: none; border: none; padding: 0;
    color: hsl(var(--primary)); cursor: pointer; font-size: inherit;
    text-decoration: underline; text-underline-offset: 2px;
  }

  /* Curated divider */
  .curated-divider { display: flex; align-items: center; gap: 0.5rem; }
  .curated-divider::before, .curated-divider::after { content: ""; flex: 1; height: 1px; background: hsl(var(--border)); }
  .curated-divider span { font-size: 0.6875rem; color: hsl(var(--muted-foreground) / 0.7); white-space: nowrap; }

  /* HF search link */
  .btn-search-hf {
    display: inline-flex; align-items: center; gap: 0.375rem;
    align-self: center; padding: 0.3rem 0.75rem;
    border: 1px solid hsl(var(--border)); border-radius: 0.5rem;
    background: transparent; color: hsl(var(--muted-foreground));
    font-size: 0.75rem; font-weight: 500; cursor: pointer;
    transition: background 120ms ease, color 120ms ease;
  }
  .btn-search-hf:hover { background: hsl(var(--muted-foreground) / 0.06); color: hsl(var(--foreground)); }

  /* Search bar */
  .search-bar { display: flex; gap: 0.375rem; align-items: center; }
  .btn-back {
    display: flex; align-items: center; justify-content: center;
    width: 2rem; height: 2rem; border-radius: 0.5rem; border: 1px solid hsl(var(--border));
    background: transparent; color: hsl(var(--muted-foreground)); cursor: pointer;
    flex-shrink: 0; transition: background 120ms ease;
  }
  .btn-back:hover { background: hsl(var(--muted-foreground) / 0.06); }
  .search-input {
    flex: 1; height: 2rem; padding: 0 0.625rem; border-radius: 0.5rem;
    border: 1px solid hsl(var(--border)); background: hsl(var(--card) / 0.6);
    color: hsl(var(--foreground)); font-size: 0.8125rem;
    outline: none; transition: border-color 120ms ease;
  }
  .search-input:focus { border-color: hsl(var(--primary) / 0.4); }
  .btn-search {
    display: flex; align-items: center; justify-content: center;
    width: 2rem; height: 2rem; border-radius: 0.5rem;
    border: none; background: hsl(var(--primary) / 0.1);
    color: hsl(var(--primary)); cursor: pointer; flex-shrink: 0;
    transition: background 120ms ease;
  }
  .btn-search:hover:not(:disabled) { background: hsl(var(--primary) / 0.18); }
  .btn-search:disabled { opacity: 0.5; cursor: not-allowed; }

  /* Search results */
  .search-result-item {
    border: 1px solid hsl(var(--border)); border-radius: 0.625rem; overflow: hidden;
    background: hsl(var(--card) / 0.6);
  }
  .search-result-item.is-expanded { border-color: hsl(var(--primary) / 0.2); }
  .search-result-header {
    display: flex; align-items: center; gap: 0.5rem; width: 100%;
    padding: 0.5rem 0.625rem; background: none; border: none;
    cursor: pointer; text-align: left;
    transition: background 120ms ease;
  }
  .search-result-header:hover:not(:disabled) { background: hsl(var(--primary) / 0.03); }
  .search-result-header:disabled { opacity: 0.45; cursor: not-allowed; }
  .badge-gated {
    font-size: 0.6rem; font-weight: 700; text-transform: uppercase;
    color: hsl(var(--warning-foreground)); background: hsl(var(--warning) / 0.1);
    padding: 0.15rem 0.4rem; border-radius: 99px; flex-shrink: 0;
  }
  .search-result-files { padding: 0 0.625rem 0.625rem; display: flex; flex-direction: column; gap: 0.25rem; }
  .files-loading { display: flex; align-items: center; justify-content: center; padding: 0.5rem 0; }
  .file-row {
    display: flex; align-items: center; gap: 0.375rem; padding: 0.375rem 0.5rem;
    border-radius: 0.4rem; border: 1px solid transparent;
    background: hsl(var(--muted-foreground) / 0.04);
    cursor: pointer; text-align: left; font-size: 0.75rem;
    color: hsl(var(--foreground)); transition: background 100ms ease, border-color 100ms ease;
  }
  .file-row:hover:not(:disabled) { background: hsl(var(--primary) / 0.05); border-color: hsl(var(--primary) / 0.12); }
  .file-row:disabled { opacity: 0.4; cursor: not-allowed; }
  .file-row.compat-fits { border-color: hsl(var(--success) / 0.2); }
  .file-row.compat-large { opacity: 0.35; }
  .file-name { flex: 1; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .file-size { font-size: 0.6875rem; color: hsl(var(--muted-foreground)); flex-shrink: 0; }
  .compat-chip {
    font-size: 0.6rem; font-weight: 700; text-transform: uppercase; letter-spacing: 0.03em;
    padding: 0.1rem 0.35rem; border-radius: 99px; flex-shrink: 0;
  }
  .compat-chip-fits { color: hsl(var(--success)); background: hsl(var(--success) / 0.1); }
  .compat-chip-might_fit { color: hsl(var(--warning-foreground)); background: hsl(var(--warning) / 0.1); }
  .compat-chip-too_large { color: hsl(var(--destructive)); background: hsl(var(--destructive) / 0.08); }

  /* Download block */
  .download-block {
    display: flex; flex-direction: column; gap: 0.375rem;
    padding: 0.625rem 0.75rem; border-radius: 0.625rem;
    border: 1px solid hsl(var(--primary) / 0.15);
    background: hsl(var(--primary) / 0.03);
  }
  .dl-header { display: flex; align-items: center; justify-content: space-between; gap: 0.5rem; }
  .dl-filename { font-size: 0.8125rem; font-weight: 500; color: hsl(var(--foreground)); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; flex: 1; }
  .btn-cancel-dl {
    display: flex; align-items: center; justify-content: center;
    width: 1.375rem; height: 1.375rem; border-radius: 50%; border: none;
    background: hsl(var(--muted-foreground) / 0.1); color: hsl(var(--muted-foreground));
    cursor: pointer; flex-shrink: 0; transition: background 120ms ease, color 120ms ease;
  }
  .btn-cancel-dl:hover { background: hsl(var(--destructive) / 0.1); color: hsl(var(--destructive)); }
  .progress-track { height: 0.25rem; border-radius: 99px; background: hsl(var(--primary) / 0.12); overflow: hidden; }
  .progress-fill {
    height: 100%; border-radius: 99px;
    background: linear-gradient(90deg, hsl(var(--primary-gradient-from)), hsl(var(--primary-gradient-to)));
    transition: width 300ms ease;
  }
  .progress-fill.indeterminate { width: 40%; animation: indeterminate 1.4s ease-in-out infinite; }
  @keyframes indeterminate { 0% { transform: translateX(-100%); } 100% { transform: translateX(350%); } }
  .dl-meta { display: flex; justify-content: space-between; font-size: 0.6875rem; color: hsl(var(--muted-foreground) / 0.75); }

  /* Errors */
  .inline-error {
    display: flex; align-items: center; gap: 0.375rem;
    font-size: 0.8125rem; color: hsl(var(--destructive));
    background: hsl(var(--destructive) / 0.05);
    border: 1px solid hsl(var(--destructive) / 0.15);
    border-radius: 0.5rem; padding: 0.5rem 0.75rem; margin: 0;
  }

  /* Footer */
  .ai-setup-footer { display: flex; flex-direction: column; align-items: center; gap: 0.5rem; padding-top: 0.25rem; }
  .btn-continue {
    display: inline-flex; align-items: center; justify-content: center; gap: 0.375rem;
    width: 100%; padding: 0.75rem 1.5rem; border-radius: 0.875rem; border: none;
    background: var(--gradient-primary); color: white;
    font-size: 0.9375rem; font-weight: 600; cursor: pointer;
    transition: opacity 150ms ease, transform 150ms ease;
    box-shadow: var(--shadow-primary-md);
  }
  .btn-continue:hover:not(:disabled) { transform: translateY(-1px); box-shadow: var(--shadow-primary-lg); }
  .btn-continue:disabled { opacity: 0.6; cursor: not-allowed; }
  .btn-skip {
    background: none; border: none; color: hsl(var(--muted-foreground) / 0.7);
    font-size: 0.8125rem; cursor: pointer; padding: 0.25rem 0.5rem; transition: color 150ms ease;
  }
  .btn-skip:hover:not(:disabled) { color: hsl(var(--muted-foreground)); }
  .btn-skip:disabled { opacity: 0.5; cursor: not-allowed; }
</style>
