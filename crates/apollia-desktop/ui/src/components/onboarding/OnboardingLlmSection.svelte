<!--
  Onboarding step 3, the language-engine half.

  Scans for GGUF weights, wires one as the default backend, and offers the three
  ways of adding another: an import from disk, the curated catalogue, and a
  HuggingFace search. The step shell holds the system information and the
  navigation; this component holds everything about language engines.
-->
<script lang="ts">
  import { get } from "svelte/store";
  import { t } from "svelte-i18n";
  import { listen } from "@tauri-apps/api/event";
  import { open as openFilePicker } from "@tauri-apps/plugin-dialog";
  import {
    cancelModelDownload,
    getHfModel,
    importModelFile,
    reloadLlm,
    scanForGgufModels,
    searchHfModels,
    setupLocalLlm,
    startModelDownload,
    type DownloadProgress,
    type GgufModelInfo,
    type HfFile,
    type HfModelCard,
    type SystemInfo,
  } from "$lib/ipc/models";
  import {
    Cpu,
    HardDrive,
    Check,
    ChevronRight,
    AlertCircle,
    Download,
    X,
    Search,
    ArrowLeft,
    Cloud,
    Upload,
  } from "lucide-svelte";
  import { Spinner, ProgressBar } from "$lib/components/ui/progress";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { llmBackends } from "$lib/stores/sse";
  import { llmSectionView, runLlmConfiguration } from "./aiSetupRules";
  import {
    CURATED_LLM_MODELS,
    modelsFitting,
    largestFitting,
    type CuratedLlmModel,
  } from "./onboardingCatalogs";
  import { dlBytes, dlPct, dlSpeed, hfFileLabelKey, pickModelsDir } from "./onboardingFormat";
  import "./onboarding-hf-search.css";

  interface Props {
    /** Drives which curated models are offered; `null` until the probe lands. */
    sysInfo: SystemInfo | null;
    /** Reports whether an engine was wired during this session. */
    onconfigured: (configured: boolean) => void;
    /** The operator asked for a cloud backend instead. */
    onopencloud: () => void;
  }

  const { sysInfo, onconfigured, onopencloud }: Props = $props();

  let ggufModels = $state<GgufModelInfo[]>([]);
  let selectedGguf = $state<GgufModelInfo | null>(null);
  let llmConfiguring = $state(false);
  let llmSuccess = $state(false);
  let llmError = $state<string | null>(null);
  let importingLlm = $state(false);

  let llmDownloadId = $state<string | null>(null);
  let llmDownloadProgress = $state<DownloadProgress | null>(null);
  let llmDownloadingModel = $state<CuratedLlmModel | HfFile | null>(null);
  let llmDownloadError = $state<string | null>(null);

  let showSearch = $state(false);
  let searchQuery = $state("");
  let searchLoading = $state(false);
  let searchResults = $state<HfModelCard[]>([]);
  let searchError = $state<string | null>(null);
  let expandedModel = $state<string | null>(null);
  let expandedDetail = $state<HfModelCard | null>(null);
  let expandLoading = $state(false);

  const availableLlmModels = $derived(modelsFitting(CURATED_LLM_MODELS, sysInfo));
  const recommendedLlm = $derived(largestFitting(CURATED_LLM_MODELS, sysInfo, 1));
  const llmView = $derived(llmSectionView(ggufModels.length, llmSuccess));

  $effect(() => {
    void loadData();
  });

  $effect(() => {
    let unlisten: (() => void) | undefined;
    listen<DownloadProgress>("model-download-progress", (event) => {
      const p = event.payload;
      if (p.id !== llmDownloadId) return;
      llmDownloadProgress = p;
      if (p.status === "completed") {
        const downloadedFilename = llmDownloadingModel?.filename ?? null;
        llmDownloadId = null;
        llmDownloadingModel = null;
        void (async () => {
          await loadData();
          // Auto-wire the freshly downloaded model as the default LLM backend
          // so the user can chat immediately after onboarding. Without this,
          // the download only lands the .gguf on disk and no backend is ever
          // created in system.db, leaving the first chat with no model.
          if (downloadedFilename && !llmSuccess) {
            const model = ggufModels.find((m) => m.filename === downloadedFilename);
            if (model) await selectGgufModel(model);
          }
        })();
      } else if (p.status === "cancelled" || p.status === "failed") {
        llmDownloadId = null;
        llmDownloadingModel = null;
        llmDownloadProgress = null;
        if (p.status === "failed")
          llmDownloadError = get(t)("onboarding.ai_setup.download_failed");
      }
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      unlisten?.();
    };
  });

  async function loadData(): Promise<void> {
    try {
      ggufModels = await scanForGgufModels();
    } catch {
      /* leave empty */
    }
  }

  async function selectGgufModel(model: GgufModelInfo): Promise<void> {
    // A configuration in flight is the only reason to ignore a click. Having
    // already wired an engine during this session is precisely when an
    // operator wants to switch to another one.
    if (llmConfiguring) return;
    // Held so a failed run can put back the engine that is actually wired.
    const previous = selectedGguf;
    await runLlmConfiguration(
      {
        selectedPath: previous?.path ?? null,
        configuring: llmConfiguring,
        configured: llmSuccess,
        error: llmError,
      },
      model.path,
      async (path) => {
        await setupLocalLlm(path);
        await reloadLlm();
      },
      (next) => {
        selectedGguf = next.selectedPath === model.path ? model : previous;
        llmConfiguring = next.configuring;
        llmSuccess = next.configured;
        llmError = next.error;
        onconfigured(next.configured);
      },
    );
  }

  async function loadLlmModel(): Promise<void> {
    if (importingLlm || llmDownloadId) return;
    importingLlm = true;
    llmError = null;
    try {
      const selected = await openFilePicker({
        multiple: false,
        filters: [{ name: "GGUF", extensions: ["gguf"] }],
        title: get(t)("onboarding.ai_setup.load_model_title"),
        defaultPath: await pickModelsDir(),
      });
      if (!selected) return;
      const filePath =
        typeof selected === "string" ? selected : (selected as { path: string }).path;
      const dest = await importModelFile(filePath, ["gguf"]);
      await loadData();
      const name = dest.split(/[\\/]/).pop() ?? "";
      const model = ggufModels.find((m) => m.filename === name);
      if (model) await selectGgufModel(model);
    } catch (err: unknown) {
      llmError = err instanceof Error ? err.message : String(err);
    } finally {
      importingLlm = false;
    }
  }

  /**
   * Extract `org/repo` from a HuggingFace direct-file URL.
   * Format observed: `https://huggingface.co/{org}/{repo}/resolve/{ref}/{path}`.
   * Returns `null` for any non-HF URL - the backend then handles the download
   * without auto-persisting the sampling defaults.
   */
  function extractHfRepoId(url: string): string | null {
    try {
      const u = new URL(url);
      if (u.hostname !== "huggingface.co") return null;
      const parts = u.pathname.split("/").filter(Boolean);
      if (parts.length < 2) return null;
      return `${parts[0]}/${parts[1]}`;
    } catch {
      return null;
    }
  }

  async function downloadLlmModel(model: CuratedLlmModel): Promise<void> {
    if (llmDownloadId) return;
    llmDownloadError = null;
    llmDownloadProgress = null;
    llmDownloadingModel = model;
    try {
      llmDownloadId = await startModelDownload({
        url: model.url,
        filename: model.filename,
        repo_id: extractHfRepoId(model.url),
      });
    } catch (err: unknown) {
      llmDownloadError = err instanceof Error ? err.message : String(err);
      llmDownloadingModel = null;
    }
  }

  async function cancelLlmDownload(): Promise<void> {
    if (!llmDownloadId) return;
    try {
      await cancelModelDownload(llmDownloadId);
    } catch {
      /* ignore */
    }
  }

  async function searchHf(): Promise<void> {
    if (!searchQuery.trim() || searchLoading) return;
    searchLoading = true;
    searchError = null;
    searchResults = [];
    expandedModel = null;
    expandedDetail = null;
    try {
      const data = await searchHfModels(searchQuery.trim(), 8);
      searchResults = data.models;
    } catch (err: unknown) {
      searchError = err instanceof Error ? err.message : String(err);
    } finally {
      searchLoading = false;
    }
  }

  async function expandSearchModel(repoId: string): Promise<void> {
    if (expandedModel === repoId) {
      expandedModel = null;
      expandedDetail = null;
      return;
    }
    expandedModel = repoId;
    expandedDetail = null;
    expandLoading = true;
    try {
      expandedDetail = await getHfModel(repoId);
    } catch {
      /* show what we have */
    } finally {
      expandLoading = false;
    }
  }

  async function downloadHfFile(file: HfFile): Promise<void> {
    if (llmDownloadId) return;
    llmDownloadError = null;
    llmDownloadProgress = null;
    llmDownloadingModel = file;
    try {
      llmDownloadId = await startModelDownload({
        url: file.download_url,
        filename: file.filename,
        repo_id: expandedDetail?.repo_id ?? extractHfRepoId(file.download_url),
      });
      showSearch = false;
    } catch (err: unknown) {
      llmDownloadError = err instanceof Error ? err.message : String(err);
      llmDownloadingModel = null;
    }
  }
</script>

<section class="setup-section" data-testid="llm-section">
  <div class="section-header">
    <HardDrive size={14} strokeWidth={2} class="text-primary" />
    <span class="section-title">{$t("onboarding.ai_setup.llm_section_title")}</span>
    {#if llmSuccess}
      <span class="section-badge-ok">
        <Check size={10} strokeWidth={2.5} /> {$t("onboarding.ai_setup.configured")}
      </span>
    {:else if $llmBackends.length > 0}
      <span class="section-badge-ok">
        <Check size={10} strokeWidth={2.5} /> {$t("onboarding.ai_setup.backends_count", { values: { count: $llmBackends.length } })}
      </span>
    {/if}
  </div>

  {#if llmView.showEmptyHint}
    <p class="empty-hint" data-testid="llm-empty-hint">
      {$t("onboarding.ai_setup.llm_empty_prefix")}
      <code>~/.apollia/models/</code> {$t("common.or")} <code>~/Downloads/</code>{$t("onboarding.ai_setup.llm_empty_suffix")}
      <Button variant="ghost" size="sm" class="inline-link" onclick={loadData}>{$t("onboarding.ai_setup.rescan")}</Button>.
    </p>
  {/if}

  {#if llmView.showSuccessRow}
    <div class="success-row" data-testid="llm-success">
      <div class="success-icon-sm"><Check size={13} strokeWidth={2.5} /></div>
      <span class="success-filename">{selectedGguf?.filename}</span>
    </div>
  {/if}

  {#if llmView.showDetectedList}
    <ul class="model-list" data-testid="llm-model-list">
      {#each ggufModels as model (model.path)}
        <li>
          <button
            class="model-row"
            class:is-selected={selectedGguf?.path === model.path && llmConfiguring}
            onclick={() => selectGgufModel(model)}
            disabled={llmConfiguring}
            data-testid="llm-model-row"
          >
            <div class="model-icon">
              {#if selectedGguf?.path === model.path && llmConfiguring}
                <Spinner size={13} />
              {:else}
                <Cpu size={13} strokeWidth={1.75} />
              {/if}
            </div>
            <div class="model-info">
              <span class="model-name">{model.filename}</span>
              <span class="model-meta">{model.size_human}</span>
            </div>
            {#if model.recommended}
              <span class="badge-recommended">{$t("onboarding.ai_setup.recommended")}</span>
            {/if}
            <ChevronRight size={13} class="text-muted-foreground/50" />
          </button>
        </li>
      {/each}
    </ul>
  {/if}

  {#if llmView.showAddMeans}
    <div class="load-model-row">
      <Button
        variant="default"
        size="sm"
        onclick={loadLlmModel}
        disabled={importingLlm || !!llmDownloadId}
        loading={importingLlm}
        data-testid="llm-load-model-btn"
      >
        <Upload size={12} strokeWidth={2} />
        {$t("onboarding.ai_setup.load_model")}
      </Button>
    </div>

    {#if llmDownloadId}
      <div class="download-block" data-testid="llm-download-progress">
        <div class="dl-header">
          <span class="dl-filename">
            {"filename" in (llmDownloadingModel ?? {})
              ? (llmDownloadingModel as CuratedLlmModel | HfFile).filename
              : "…"}
          </span>
          <Button variant="ghost" size="sm" class="btn-cancel-dl" onclick={cancelLlmDownload} aria-label={$t("onboarding.ai_setup.cancel_download")}>
            <X size={12} strokeWidth={2} />
          </Button>
        </div>
        <ProgressBar
          value={llmDownloadProgress ? dlPct(llmDownloadProgress) : undefined}
          size="sm"
          variant="primary"
        />
        {#if llmDownloadProgress}
          <div class="dl-meta">
            <span>{dlBytes(llmDownloadProgress)}</span>
            <span>{dlSpeed(llmDownloadProgress.speed_bps)}</span>
          </div>
        {/if}
      </div>
    {:else if showSearch}
      <div class="search-bar">
        <Button variant="ghost" size="sm" class="btn-back" onclick={() => { showSearch = false; searchResults = []; }} aria-label={$t("common.back")}>
          <ArrowLeft size={12} strokeWidth={2} />
        </Button>
        <Input
          class="search-input"
          type="text"
          placeholder={$t("onboarding.ai_setup.search_placeholder")}
          aria-label={$t("onboarding.ai_setup.search_placeholder")}
          bind:value={searchQuery}
          onkeydown={(e) => e.key === "Enter" && searchHf()}
        />
        <Button variant="ghost" size="sm" class="btn-search" onclick={searchHf} disabled={searchLoading} aria-label={$t("onboarding.ai_setup.search")}>
          {#if searchLoading}<Spinner size={12} />{:else}<Search size={12} strokeWidth={2} />{/if}
        </Button>
      </div>

      {#if searchError}
        <p class="inline-error" role="alert"><AlertCircle size={12} />{searchError}</p>
      {/if}

      {#if searchResults.length > 0}
        <ul class="model-list" data-testid="search-results">
          {#each searchResults as model (model.repo_id)}
            {@const isExpanded = expandedModel === model.repo_id}
            {@const detail = isExpanded ? expandedDetail : null}
            <li class="search-result-item" class:is-expanded={isExpanded}>
              <button
                type="button"
                class="search-result-header"
                onclick={() => expandSearchModel(model.repo_id)}
                disabled={model.compatibility_issue === "embedding_model" || model.compatibility_issue === "no_gguf_files"}
              >
                <div class="model-icon">
                  <Cpu size={12} strokeWidth={1.75} />
                </div>
                <span class="model-name">{model.repo_id}</span>
                {#if model.gated}
                  <span class="badge-gated">{$t("onboarding.ai_setup.gated")}</span>
                {/if}
                <ChevronRight
                  size={12}
                  class="text-muted-foreground/50 transition-transform {isExpanded ? 'rotate-90' : ''}"
                />
              </button>

              {#if isExpanded}
                <div class="search-result-files">
                  {#if expandLoading && !detail}
                    <div class="files-loading"><Spinner size={12} /></div>
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
                        <Download size={10} strokeWidth={2} />
                        <span class="file-name">{file.filename}</span>
                        <span class="file-size">{file.size_human}</span>
                        {#if file.compatibility}
                          {@const compatKey = hfFileLabelKey(file)}
                          <span class="compat-chip compat-chip-{file.compatibility}">
                            {compatKey ? $t(compatKey) : ""}
                          </span>
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
          {searchQuery
            ? $t("onboarding.ai_setup.search_no_results")
            : $t("onboarding.ai_setup.search_prompt")}
        </p>
      {/if}
    {:else}
      <div class="curated-divider"><span>{$t("onboarding.ai_setup.recommended_models")}</span></div>
      <ul class="model-list" data-testid="curated-llm-list">
        {#each availableLlmModels as model (model.filename)}
          <li>
            <button type="button" class="model-row" onclick={() => downloadLlmModel(model)} data-testid="curated-llm-row">
              <div class="model-icon"><Download size={12} strokeWidth={1.75} /></div>
              <div class="model-info">
                <span class="model-name">{model.name}</span>
                <span class="model-meta">{model.size_label}</span>
              </div>
              {#if model.filename === recommendedLlm?.filename}
                <span class="badge-recommended">{$t("onboarding.ai_setup.recommended")}</span>
              {/if}
              <ChevronRight size={13} class="text-muted-foreground/50" />
            </button>
          </li>
        {/each}
      </ul>
      <div class="alt-row">
        <Button variant="ghost" size="sm" class="btn-search-hf" onclick={() => { showSearch = true; searchResults = []; }}>
          <Search size={11} strokeWidth={2} />
          {$t("onboarding.ai_setup.search_hf")}
        </Button>
        <Button variant="ghost" size="sm" class="btn-search-hf btn-cloud" onclick={onopencloud} data-testid="onboarding-open-cloud">
          <Cloud size={11} strokeWidth={2} />
          {$t("onboarding.ai_setup.use_cloud")}
        </Button>
      </div>
    {/if}

  {/if}

  {#if llmDownloadError}
    <p class="inline-error" role="alert" data-testid="llm-download-error">
      <AlertCircle size={12} />{llmDownloadError}
    </p>
  {/if}

  {#if llmError}
    <p class="inline-error" role="alert" data-testid="llm-error"><AlertCircle size={12} />{llmError}</p>
  {/if}
</section>
