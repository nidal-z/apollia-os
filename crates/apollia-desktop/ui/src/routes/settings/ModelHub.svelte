<script lang="ts">
  /**
   * Model Hub - browse, download, and manage local GGUF models.
   *
   * An action-oriented settings sub-page (no explicit-save footer): a hardware
   * profile, the installed-models manager, the live download queue, and a
   * HuggingFace search with per-variant fit verdicts. State and IPC live here;
   * each section renders through a `model-hub/*` subcomponent.
   */
  import { onMount, onDestroy } from "svelte";
  import { t } from "svelte-i18n";
  import type { UnlistenFn } from "@tauri-apps/api/event";
  import { addToast } from "$lib/components/ui/toast";
  import ConfirmDialog from "$lib/components/ui/dialog/ConfirmDialog.svelte";
  import { reportError } from "$lib/errors/reportError";
  import type { HumanizedError } from "$lib/errors/humanize";
  import SettingsSubPage from "../../components/settings/SettingsSubPage.svelte";
  import HardwareCard from "../../components/settings/model-hub/HardwareCard.svelte";
  import InstalledModelsCard from "../../components/settings/model-hub/InstalledModelsCard.svelte";
  import ActiveDownloadsCard from "../../components/settings/model-hub/ActiveDownloadsCard.svelte";
  import HfTokenForm from "../../components/settings/model-hub/HfTokenForm.svelte";
  import ModelFilterBar from "../../components/settings/model-hub/ModelFilterBar.svelte";
  import ModelResults from "../../components/settings/model-hub/ModelResults.svelte";
  import {
    getHardwareProfile,
    searchHfModels,
    getHfModel,
    startModelDownload,
    cancelModelDownload,
    listInstalledModels,
    deleteInstalledModel,
    listenDownloadProgress,
    type HardwareProfile,
    type HfModelCard,
    type HfFile,
    type DownloadProgress,
    type InstalledModel,
  } from "$lib/ipc/modelHub";
  import {
    isBlockingIssue,
    licenseTypeOf,
    modelCategoryOf,
    type GgufGroup,
    type LicenseFilter,
    type ModelTypeFilter,
    type GatedFilter,
  } from "../../components/settings/model-hub/helpers";

  const PAGE_LIMIT = 50;

  let hardware = $state<HardwareProfile | null>(null);
  let hardwareLoading = $state(true);

  let searchQuery = $state("");
  let searchResults = $state<HfModelCard[]>([]);
  let searching = $state(false);
  let searchError = $state<HumanizedError | null>(null);
  let hasSearched = $state(false);
  let nextCursor = $state<string | null>(null);
  let hasMore = $state(false);

  let expandedModel = $state<string | null>(null);
  let modelDetail = $state<HfModelCard | null>(null);
  let modelDetailLoading = $state(false);

  let activeDownloads = $state<Map<string, DownloadProgress>>(new Map());

  let hfToken = $state("");
  let showTokenForm = $state(false);

  let sortBy = $state("downloads");
  let langFilter = $state("");
  let licenseFilter = $state<LicenseFilter>("any");
  let modelTypeFilter = $state<ModelTypeFilter>("any");
  let gatedFilter = $state<GatedFilter>("any");
  let showIncompatible = $state(false);

  let installedModels = $state<InstalledModel[]>([]);
  let installedLoading = $state(true);
  let deleteTarget = $state<InstalledModel | null>(null);
  let deleting = $state(false);

  let unlisten: UnlistenFn | null = null;

  const downloadList = $derived([...activeDownloads.values()]);

  const filteredResults = $derived(
    searchResults
      .filter((m) => showIncompatible || !isBlockingIssue(m.compatibility_issue))
      .filter((m) => licenseFilter === "any" || licenseTypeOf(m) === licenseFilter)
      .filter((m) => {
        if (modelTypeFilter === "any") return true;
        const cat = modelCategoryOf(m);
        if (modelTypeFilter === "base") return cat === "base" || cat === "unknown";
        return cat === modelTypeFilter;
      })
      .filter((m) =>
        gatedFilter === "any" ? true : gatedFilter === "open" ? !m.gated : m.gated,
      ),
  );

  onMount(async () => {
    await loadHardware();
    await loadInstalledModels();
    await search();

    unlisten = await listenDownloadProgress((p) => {
      const next = new Map(activeDownloads);
      if (p.status === "completed" || p.status === "cancelled" || p.status === "failed") {
        next.delete(p.id);
        if (p.status === "completed") {
          addToast($t("settings.model_hub.downloads.completed_toast", { values: { path: p.dest_path } }), "success");
          void loadInstalledModels();
        } else if (p.status === "failed") {
          addToast($t("settings.model_hub.downloads.failed_toast"), "error");
        }
      } else {
        next.set(p.id, p);
      }
      activeDownloads = next;
    });

    window.addEventListener("keydown", onGlobalKeydown);
  });

  onDestroy(() => {
    unlisten?.();
    window.removeEventListener("keydown", onGlobalKeydown);
  });

  function onGlobalKeydown(event: KeyboardEvent): void {
    if (event.key !== "/" || event.metaKey || event.ctrlKey || event.altKey) return;
    const el = event.target as HTMLElement | null;
    const tag = el?.tagName;
    if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT" || el?.isContentEditable) return;
    event.preventDefault();
    document.getElementById("model-hub-search-input")?.focus();
  }

  async function loadHardware(): Promise<void> {
    hardwareLoading = true;
    try {
      hardware = await getHardwareProfile();
    } catch (e) {
      reportError(e, { surface: "toast" });
    } finally {
      hardwareLoading = false;
    }
  }

  async function search(append = false): Promise<void> {
    searching = true;
    searchError = null;
    hasSearched = true;
    try {
      const result = await searchHfModels({
        query: searchQuery,
        limit: PAGE_LIMIT,
        sort: sortBy,
        pipeline_tag: "text-generation",
        language: langFilter || null,
        next_cursor: append ? nextCursor : null,
        hf_token: hfToken || null,
      });
      searchResults = append ? [...searchResults, ...result.models] : result.models;
      nextCursor = result.next_cursor;
      hasMore = result.next_cursor !== null;
    } catch (e) {
      searchError = reportError(e, { surface: "inline" });
    } finally {
      searching = false;
    }
  }

  async function expandModel(repoId: string): Promise<void> {
    if (expandedModel === repoId) {
      expandedModel = null;
      modelDetail = null;
      return;
    }
    expandedModel = repoId;
    modelDetail = null;
    modelDetailLoading = true;
    try {
      modelDetail = await getHfModel(repoId, hfToken || null);
    } catch (e) {
      reportError(e, { surface: "toast" });
      expandedModel = null;
    } finally {
      modelDetailLoading = false;
    }
  }

  async function startDownload(file: HfFile): Promise<void> {
    const card = modelDetail ?? searchResults.find((m) => m.repo_id === expandedModel);
    if (card?.gated && !hfToken) {
      showTokenForm = true;
      return;
    }
    try {
      await startModelDownload({
        url: file.download_url,
        filename: file.filename,
        hf_token: hfToken || null,
        repo_id: card?.repo_id ?? null,
      });
      addToast($t("settings.model_hub.downloads.downloading_toast", { values: { filename: file.filename } }), "info");
    } catch (e) {
      reportError(e, { surface: "toast" });
    }
  }

  async function downloadGroup(group: GgufGroup): Promise<void> {
    for (const file of group.files) await startDownload(file);
  }

  async function cancelDownload(id: string): Promise<void> {
    try {
      await cancelModelDownload(id);
    } catch (e) {
      reportError(e, { surface: "toast" });
    }
  }

  async function loadInstalledModels(): Promise<void> {
    installedLoading = true;
    try {
      installedModels = await listInstalledModels();
    } catch (e) {
      reportError(e, { surface: "toast" });
    } finally {
      installedLoading = false;
    }
  }

  async function confirmDeleteModel(): Promise<void> {
    if (!deleteTarget) return;
    deleting = true;
    try {
      await deleteInstalledModel(deleteTarget.path);
      addToast($t("settings.model_hub.installed.deleted_toast", { values: { name: deleteTarget.name } }), "success");
      deleteTarget = null;
      await loadInstalledModels();
    } catch (e) {
      reportError(e, { surface: "toast" });
    } finally {
      deleting = false;
    }
  }

  function clearFilters(): void {
    licenseFilter = "any";
    modelTypeFilter = "any";
    gatedFilter = "any";
    showIncompatible = true;
  }
</script>

<SettingsSubPage route="model-hub" data-testid="model-hub-page">
  <HardwareCard {hardware} loading={hardwareLoading} />

  <InstalledModelsCard
    models={installedModels}
    loading={installedLoading}
    onRequestDelete={(m) => (deleteTarget = m)}
  />

  {#if downloadList.length > 0}
    <ActiveDownloadsCard downloads={downloadList} onCancel={cancelDownload} />
  {/if}

  {#if showTokenForm}
    <HfTokenForm bind:token={hfToken} onDone={() => (showTokenForm = false)} />
  {/if}

  <ModelFilterBar
    bind:searchQuery
    bind:sortBy
    bind:langFilter
    bind:licenseFilter
    bind:modelTypeFilter
    bind:gatedFilter
    bind:showIncompatible
    {searching}
    tokenSet={hfToken.length > 0}
    filteredCount={filteredResults.length}
    totalCount={searchResults.length}
    onSearch={() => search()}
    onToggleToken={() => (showTokenForm = !showTokenForm)}
  />

  <ModelResults
    results={filteredResults}
    totalCount={searchResults.length}
    {searching}
    {hasSearched}
    {searchQuery}
    {searchError}
    {hasMore}
    {expandedModel}
    detail={modelDetail}
    detailLoading={modelDetailLoading}
    downloads={downloadList}
    onToggle={expandModel}
    onDownloadGroup={downloadGroup}
    onDownloadFile={startDownload}
    onLoadMore={() => search(true)}
    onRetry={() => search()}
    onClearFilters={clearFilters}
  />
</SettingsSubPage>

<ConfirmDialog
  open={deleteTarget !== null}
  onclose={() => (deleteTarget = null)}
  onconfirm={confirmDeleteModel}
  title={$t("settings.model_hub.installed.delete_title")}
  message={$t("settings.model_hub.installed.delete_message", { values: { name: deleteTarget?.name ?? "" } })}
  confirmLabel={$t("settings.model_hub.installed.delete")}
  cancelLabel={$t("common.cancel")}
  loading={deleting}
  data-testid="installed-model-delete-dialog"
/>
