<!--
  Onboarding step 3, the language-engine half.

  Scans for GGUF weights, wires one as the default backend, and offers the three
  ways of adding another: an import from disk, the curated catalogue, and a
  HuggingFace search. The step shell holds the system information and the
  navigation; this component holds everything about language engines.
-->
<script lang="ts">
  import { onMount } from "svelte";
  import { get } from "svelte/store";
  import { t } from "svelte-i18n";
  import { listen } from "@tauri-apps/api/event";
  import { open as openFilePicker } from "@tauri-apps/plugin-dialog";
  import {
    cancelModelDownload,
    getHfModel,
    importModelFile,
    reloadLlm,
    recommendModels,
    scanForGgufModels,
    searchHfModels,
    setupLocalLlm,
    startModelDownload,
    type DownloadProgress,
    type GgufModelInfo,
    type HfFile,
    type HfModelCard,
    type RecommendOutcome,
    type RecommendedModel,
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
  import { Select } from "$lib/components/ui/select";
  import {
    CONTEXT_WINDOW_CHOICES,
    DEFAULT_CONTEXT_WINDOW,
    formatContextWindow,
  } from "$lib/contextWindow";
  import { llmBackends } from "$lib/stores/sse";
  import { llmSectionView, runLlmConfiguration } from "./aiSetupRules";
  import {
    defaultChoice,
    measuredLabel,
    memoryLabel,
    modelDisplayName,
    needsOffloadWarning,
    primaryReason,
    rowCaveats,
    sizeLabel,
    supportsToolCalling,
  } from "./onboardingRecommendations";
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
  let llmDownloadingModel = $state<{ filename: string } | HfFile | null>(null);
  let llmDownloadError = $state<string | null>(null);

  // `null` keeps the backend's own default (`~/.apollia/models`); a masterised
  // Windows profile with a constrained `C:` drive is the reason this exists.
  let destDir = $state<string | null>(null);

  let showSearch = $state(false);
  let searchQuery = $state("");
  let searchLoading = $state(false);
  let searchResults = $state<HfModelCard[]>([]);
  let searchError = $state<string | null>(null);
  let expandedModel = $state<string | null>(null);
  let expandedDetail = $state<HfModelCard | null>(null);
  let expandLoading = $state(false);

  // The catalogue is fetched live, so the step has four states rather than a
  // list: still measuring, ranked, HuggingFace unreachable, and reachable but
  // nothing fits. The last two look identical to a component that only ever
  // receives an array, and they call for opposite advice.
  let recommendation = $state<RecommendOutcome | null>(null);
  let recommendLoading = $state(false);
  // Bumped by every request, so an answer computed for a window the operator
  // has since changed is dropped instead of shown.
  let recommendRequest = 0;

  // The window the engine will be launched with. It sizes the cache the
  // recommender reserves, so it is chosen here, before a model, and stored on
  // the backend the step wires.
  let contextWindow = $state(String(DEFAULT_CONTEXT_WINDOW));

  const recommendedModels = $derived(
    recommendation?.status === "ok" ? recommendation.models : [],
  );
  const topRecommendation = $derived(defaultChoice(recommendedModels));
  const llmView = $derived(llmSectionView(ggufModels.length, llmSuccess));

  // Once, on mount, and deliberately not an `$effect`. `loadRecommendations`
  // reads `recommendLoading` before its first await, which an effect records as
  // a dependency: every completed request flipped it back to false, re-ran the
  // effect, and started a full HuggingFace resolution again. The spinner never
  // settled, which is what an operator saw as an analysis that loaded forever.
  onMount(() => {
    void loadData();
    void loadRecommendations();
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

  async function loadRecommendations(): Promise<void> {
    const request = ++recommendRequest;
    recommendLoading = true;
    try {
      const outcome = await recommendModels({ n_ctx: Number(contextWindow) });
      if (request !== recommendRequest) return;
      recommendation = outcome;
    } catch (err: unknown) {
      if (request !== recommendRequest) return;
      // The command only fails when the hardware probe itself does. Treated as
      // "could not look" rather than "nothing fits", for the same reason the
      // backend keeps those two outcomes apart.
      recommendation = {
        status: "unreachable",
        detail: err instanceof Error ? err.message : String(err),
        hardware: {
          total_ram_gb: sysInfo?.total_ram_gb ?? 0,
          available_ram_gb: sysInfo?.available_ram_gb ?? 0,
          cpu_model: "",
          cpu_cores: 0,
          memory_budget_gb: 0,
          accelerator: { kind: "none" },
        },
      };
    } finally {
      if (request === recommendRequest) recommendLoading = false;
    }
  }

  async function onContextWindowChange(): Promise<void> {
    void loadRecommendations();
    // An engine already wired in this step takes the new window at once,
    // rather than the one it was set up with a moment ago.
    if (llmSuccess && selectedGguf && !llmConfiguring) {
      try {
        await setupLocalLlm(selectedGguf.path, Number(contextWindow));
        await reloadLlm();
      } catch (err: unknown) {
        llmError = err instanceof Error ? err.message : String(err);
      }
    }
  }

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
        await setupLocalLlm(path, Number(contextWindow));
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

  async function chooseDestDir(): Promise<void> {
    const selected = await openFilePicker({
      directory: true,
      defaultPath: destDir ?? (await pickModelsDir()),
    });
    if (!selected) return;
    destDir = typeof selected === "string" ? selected : (selected as { path: string }).path;
  }

  async function downloadLlmModel(model: RecommendedModel): Promise<void> {
    if (llmDownloadId) return;
    llmDownloadError = null;
    llmDownloadProgress = null;
    llmDownloadingModel = { filename: model.file.filename };
    try {
      llmDownloadId = await startModelDownload({
        url: model.file.download_url,
        filename: model.file.filename,
        // The repository is known from the resolution rather than parsed back
        // out of the URL, so the downloader can fetch the publisher's own
        // generation_config.json afterwards.
        repo_id: model.file.repo_id || extractHfRepoId(model.file.download_url),
        dest_dir: destDir,
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
        dest_dir: destDir,
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

    <div class="dest-dir-row" data-testid="llm-context-window-row">
      <label class="dest-dir-label" for="onboarding-context-window">
        {$t("onboarding.ai_setup.context_window_label")}
      </label>
      <Select
        id="onboarding-context-window"
        size="sm"
        class="context-window-select"
        bind:value={contextWindow}
        onchange={() => void onContextWindowChange()}
        data-testid="llm-context-window"
      >
        {#each CONTEXT_WINDOW_CHOICES as n (n)}
          <option value={String(n)}>{formatContextWindow(n)}</option>
        {/each}
      </Select>
    </div>
    <p class="dest-dir-label context-window-hint">{$t("onboarding.ai_setup.context_window_hint")}</p>

    <div class="dest-dir-row" data-testid="llm-dest-dir-row">
      <span class="dest-dir-label">
        {destDir
          ? $t("onboarding.ai_setup.dest_dir_custom", { values: { path: destDir } })
          : $t("onboarding.ai_setup.dest_dir_default")}
      </span>
      <Button variant="ghost" size="sm" class="inline-link" onclick={chooseDestDir} data-testid="llm-dest-dir-change">
        {$t("onboarding.ai_setup.dest_dir_change")}
      </Button>
    </div>

    {#if llmDownloadId}
      <div class="download-block" data-testid="llm-download-progress">
        <div class="dl-header">
          <span class="dl-filename">
            {llmDownloadingModel?.filename ?? "…"}
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

      {#if recommendLoading}
        <div class="recommend-state" data-testid="llm-recommend-loading">
          <Spinner size={14} />
          <span>{$t("onboarding.ai_setup.recommend_loading")}</span>
        </div>
      {:else if recommendation?.status === "unreachable"}
        <div class="recommend-state recommend-state-warn" data-testid="llm-recommend-unreachable">
          <AlertCircle size={13} strokeWidth={1.75} />
          <div class="recommend-state-text">
            <span class="recommend-state-title">{$t("onboarding.ai_setup.recommend_unreachable_title")}</span>
            <span class="recommend-state-body">{$t("onboarding.ai_setup.recommend_unreachable_body")}</span>
          </div>
          <Button variant="ghost" size="sm" onclick={() => void loadRecommendations()} data-testid="llm-recommend-retry">
            {$t("onboarding.ai_setup.recommend_retry")}
          </Button>
        </div>
      {:else if recommendation?.status === "empty"}
        <div class="recommend-state recommend-state-warn" data-testid="llm-recommend-empty">
          <AlertCircle size={13} strokeWidth={1.75} />
          <div class="recommend-state-text">
            <span class="recommend-state-title">{$t("onboarding.ai_setup.recommend_empty_title")}</span>
            <span class="recommend-state-body">{$t("onboarding.ai_setup.recommend_empty_body")}</span>
          </div>
        </div>
      {:else}
        <ul class="model-list" data-testid="curated-llm-list">
          {#each recommendedModels as model (model.file.download_url)}
            {@const reason = primaryReason(model)}
            {@const memory = memoryLabel(model)}
            {@const caveats = rowCaveats(model)}
            <li>
              <button type="button" class="model-row" onclick={() => downloadLlmModel(model)} data-testid="curated-llm-row">
                <div class="model-icon"><Download size={12} strokeWidth={1.75} /></div>
                <div class="model-info">
                  <span class="model-name">
                    {modelDisplayName(model)}
                    {#if model.quant}<span class="model-quant">{model.quant}</span>{/if}
                  </span>
                  <span class="model-meta">
                    {sizeLabel(model.file.size_bytes)} · {$t(memory.key, { values: memory.values })}
                  </span>
                  {#if reason}
                    <span class="model-reason">{$t(reason.key, { values: reason.values })}</span>
                  {/if}
                  {#each caveats as caveat (caveat.key)}
                    <span class="model-caveat">{$t(caveat.key, { values: caveat.values })}</span>
                  {/each}
                </div>
                {#if needsOffloadWarning(model)}
                  <span class="badge-no-tools">
                    {$t("onboarding.ai_setup.reason_partial_offload", {
                      values: {
                        gpu: model.offload.n_gpu_layers,
                        total: model.offload.total_layers,
                      },
                    })}
                  </span>
                {/if}
                {#if !supportsToolCalling(model)}
                  <span class="badge-no-tools">{$t("onboarding.ai_setup.no_tool_calling_badge")}</span>
                {/if}
                {#if model.file.download_url === topRecommendation?.file.download_url}
                  <span class="badge-recommended">{$t("onboarding.ai_setup.recommended")}</span>
                {/if}
                <ChevronRight size={13} class="text-muted-foreground/50" />
              </button>
            </li>
          {/each}
        </ul>
        {#if recommendation?.status === "ok" && recommendation.hardware.cpu_model}
          {@const measured = measuredLabel(recommendation.hardware)}
          <p class="recommend-measured" data-testid="llm-recommend-measured">
            {$t(measured.key, { values: measured.values })}
          </p>
        {/if}
      {/if}
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
