<!--
  First-launch step 2, the language engine.

  Scans for GGUF weights, wires one as the default backend, and offers the three
  ways of adding another: an import from disk, the curated catalogue, and a
  HuggingFace search. The step shell holds the system information and the
  navigation; this component holds everything about language engines.
-->
<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { get } from "svelte/store";
  import { t } from "svelte-i18n";
  import { open as openFilePicker } from "@tauri-apps/plugin-dialog";
  import {
    getHfModel,
    importModelFile,
    reloadLlm,
    recommendModels,
    scanForGgufModels,
    searchHfModels,
    setupLocalLlm,
    startModelDownload,
    type GgufModelInfo,
    type HfFile,
    type HfModelCard,
    type RecommendOutcome,
    type RecommendedModel,
    type SystemInfo,
  } from "$lib/ipc/models";
  import { startVariantDownload } from "$lib/ipc/modelHub";
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
    Server,
    Upload,
  } from "lucide-svelte";
  import { Spinner, ProgressBar } from "$lib/components/ui/progress";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { llmBackends } from "$lib/stores/sse";
  import {
    LANGUAGE_DATA_EXTENSION,
    isLanguageData,
    llmSectionView,
    runLlmConfiguration,
  } from "./aiSetupRules";
  import { isInFlight } from "./llmDownload";
  import { llmDownload } from "./llmDownloadLive";
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
    /**
     * Reports whether an engine was wired during this session, and the size of
     * its file, which the next step sizes its own recommendation beside.
     */
    onconfigured: (configured: boolean, sizeBytes: number | null) => void;
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
  /** File name of the recognition language data the last import brought in. */
  let languageDataImported = $state<string | null>(null);

  // The download is followed outside this component: the step may be left
  // while it runs, and the file is still wired as the engine when it lands.
  const llmDownloading = $derived(isInFlight($llmDownload));
  const llmDownloadProgress = $derived($llmDownload.progress);
  const llmDownloadError = $derived.by(() => {
    const failure = $llmDownload.failure;
    if (!failure) return null;
    return failure.detail ?? $t("onboarding.ai_setup.download_failed");
  });

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
  // Bumped by every request, so an answer a retry has since replaced is
  // dropped instead of shown.
  let recommendRequest = 0;

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

  // A file the download wired as the engine, while this step was open or
  // after it was left and reopened: the section names it as its own.
  $effect(() => {
    const path = $llmDownload.wiredPath;
    if (!path) return;
    untrack(() => void adoptWiredModel(path));
  });

  async function adoptWiredModel(path: string): Promise<void> {
    await loadData();
    const model = ggufModels.find((m) => m.path === path);
    if (!model) return;
    selectedGguf = model;
    llmSuccess = true;
    onconfigured(true, model.size_bytes);
  }

  async function loadRecommendations(): Promise<void> {
    const request = ++recommendRequest;
    recommendLoading = true;
    try {
      // No window is passed: the recommendation and the engine both follow
      // the default of the model.
      const outcome = await recommendModels();
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
          total_capacity_gb: 0,
          accelerator: { kind: "none" },
        },
        selection: [],
      };
    } finally {
      if (request === recommendRequest) recommendLoading = false;
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
        await setupLocalLlm(path);
        await reloadLlm();
        // An engine chosen by hand is kept when a download lands after it.
        llmDownload.keepCurrentEngine();
      },
      (next) => {
        selectedGguf = next.selectedPath === model.path ? model : previous;
        llmConfiguring = next.configuring;
        llmSuccess = next.configured;
        llmError = next.error;
        onconfigured(next.configured, next.configured ? (selectedGguf?.size_bytes ?? null) : null);
      },
    );
  }

  async function loadLlmModel(): Promise<void> {
    if (importingLlm || llmDownloading) return;
    importingLlm = true;
    llmError = null;
    languageDataImported = null;
    try {
      const selected = await openFilePicker({
        multiple: false,
        filters: [
          { name: "GGUF", extensions: ["gguf"] },
          {
            name: get(t)("onboarding.ai_setup.load_language_data_filter"),
            extensions: [LANGUAGE_DATA_EXTENSION],
          },
        ],
        title: get(t)("onboarding.ai_setup.load_model_title"),
        defaultPath: await pickModelsDir(),
      });
      if (!selected) return;
      const filePath =
        typeof selected === "string" ? selected : (selected as { path: string }).path;
      const dest = await importModelFile(filePath, ["gguf"]);
      const name = dest.split(/[\\/]/).pop() ?? "";
      if (isLanguageData(dest)) {
        languageDataImported = name;
        return;
      }
      await loadData();
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
    // A recommendation can be a split model, named by its first part: the
    // variant download fetches every part, and the projector when the
    // repository publishes one. The resolution always names the repository;
    // an empty one is refused by the backend with its own error.
    await llmDownload.start(
      () =>
        startVariantDownload({
          repo_id: model.file.repo_id || extractHfRepoId(model.file.download_url) || "",
          selector: { kind: "gguf", file: model.file.filename },
          hf_token: null,
          dest_dir: destDir,
        }),
      model.file.filename,
      model.file.size_bytes,
    );
  }

  async function cancelLlmDownload(): Promise<void> {
    await llmDownload.cancel();
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
    await llmDownload.start(
      () =>
        startModelDownload({
          url: file.download_url,
          filename: file.filename,
          repo_id: expandedDetail?.repo_id ?? extractHfRepoId(file.download_url),
          dest_dir: destDir,
        }),
      file.filename,
      file.size_bytes,
    );
    if (get(llmDownload).phase === "downloading") showSearch = false;
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
        disabled={importingLlm || llmDownloading}
        loading={importingLlm}
        data-testid="llm-load-model-btn"
      >
        <Upload size={12} strokeWidth={2} />
        {$t("onboarding.ai_setup.load_model")}
      </Button>
    </div>

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

    {#if llmDownloading}
      <div class="download-block" data-testid="llm-download-progress">
        <div class="dl-header">
          <span class="dl-filename">
            {$llmDownload.filename ?? "…"}
          </span>
          <Button variant="ghost" size="sm" class="btn-cancel-dl" onclick={cancelLlmDownload} aria-label={$t("onboarding.ai_setup.cancel_download")} data-testid="llm-download-cancel">
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
          <Server size={11} strokeWidth={2} />
          {$t("onboarding.ai_setup.connect_server")}
        </Button>
      </div>
    {/if}

  {/if}

  {#if llmDownloadError}
    <p class="inline-error" role="alert" data-testid="llm-download-error">
      <AlertCircle size={12} />{llmDownloadError}
    </p>
  {/if}

  {#if languageDataImported}
    <p class="empty-hint" role="status" data-testid="llm-language-data-imported">
      {$t("onboarding.ai_setup.language_data_imported", { values: { file: languageDataImported } })}
    </p>
  {/if}

  {#if llmError}
    <p class="inline-error" role="alert" data-testid="llm-error"><AlertCircle size={12} />{llmError}</p>
  {/if}
</section>
