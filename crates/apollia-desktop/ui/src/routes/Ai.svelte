<script lang="ts">
  /**
   * Ai - the AI screen: the models resident in memory and the models of this
   * computer, by category, with the default model of the machine.
   *
   * The servers, the catalogue and the panel of a model are not here yet; the
   * management of the engines stays in Settings until the screen takes them.
   */
  import { onMount } from "svelte";
  import { get } from "svelte/store";
  import { t } from "svelte-i18n";
  import { open as openFilePicker } from "@tauri-apps/plugin-dialog";
  import type { LoadedModel, LoadedModelsResponse } from "$lib/types";
  import {
    deleteInstalledModel,
    listInstalledModels,
    type InstalledModel,
  } from "$lib/ipc/modelHub";
  import { listLoadedModels, reloadLlmFromDb, unloadLoadedModel } from "$lib/ipc/llm";
  import { listModelPins, setModelPin } from "$lib/ipc/llmLoad";
  import { getModelsDir } from "$lib/ipc/modelsFolder";
  import { importModelFile } from "$lib/ipc/models";
  import {
    revealInstalledModel,
    setDefaultModel,
    setEmbeddingsDefault,
    setInstalledModelPinned,
  } from "$lib/ipc/aiModels";
  import { openSettingsWindow } from "$lib/stores/settingsWindow";
  import { openExternalUrl } from "$lib/utils/externalLink";
  import { reportError } from "$lib/errors/reportError";
  import { addToast } from "$lib/components/ui/toast";
  import ConfirmDialog from "$lib/components/ui/dialog/ConfirmDialog.svelte";
  import { cn } from "$lib/utils";
  import LoadedModelsSection from "../components/ai/LoadedModelsSection.svelte";
  import InstalledModelsSection from "../components/ai/InstalledModelsSection.svelte";
  import { unloadErrorKey } from "../components/ai/loadedModels";
  import {
    CATEGORY_FILTERS,
    categoryCount,
    categoryLabelKey,
    hostPlatform,
    isFilterShortcut,
    type CategoryFilter,
    type MenuEntryId,
  } from "../components/ai/aiModels";

  const platform = hostPlatform();

  let installed = $state<InstalledModel[]>([]);
  let installedError = $state<string | null>(null);
  let loaded = $state<LoadedModelsResponse | null>(null);
  let loadedError = $state<string | null>(null);
  let modelsDir = $state<string | null>(null);
  let filter = $state<CategoryFilter>("all");
  let query = $state("");
  let unloading = $state<string | null>(null);
  let pins = $state<string[]>([]);
  let pinning = $state<string | null>(null);
  let importing = $state(false);
  let searchTarget = $state<InstalledModel | null>(null);
  let deleteTarget = $state<InstalledModel | null>(null);
  let working = $state(false);

  const loadedPaths = $derived(new Set((loaded?.models ?? []).map((m) => m.modelPath)));

  async function loadInstalled(): Promise<void> {
    try {
      installed = await listInstalledModels();
      installedError = null;
    } catch (err: unknown) {
      installedError = err instanceof Error ? err.message : String(err);
    }
  }

  async function loadLoaded(): Promise<void> {
    try {
      loaded = await listLoadedModels();
      loadedError = null;
    } catch (err: unknown) {
      loadedError = err instanceof Error ? err.message : String(err);
    }
  }

  async function loadPins(): Promise<void> {
    try {
      pins = (await listModelPins()).pins;
    } catch {
      // Without a local engine there is nothing to keep in memory.
      pins = [];
    }
  }

  /** Keeps a loaded model in memory, or stops keeping it. */
  async function pin(model: LoadedModel, pinned: boolean): Promise<void> {
    if (pinning) return;
    pinning = model.modelPath;
    const name = model.modelId ?? model.model;
    try {
      await setModelPin(model.modelPath, pinned);
      addToast(
        get(t)(pinned ? "ai.loaded.pinned_toast" : "ai.loaded.unpinned_toast", { values: { name } }),
        "success",
      );
    } catch (err: unknown) {
      reportError(err, { surface: "toast" });
    } finally {
      pinning = null;
      await loadPins();
    }
  }

  async function loadFolder(): Promise<void> {
    try {
      modelsDir = await getModelsDir();
    } catch {
      modelsDir = null;
    }
  }

  async function refresh(): Promise<void> {
    await Promise.all([loadInstalled(), loadLoaded(), loadPins(), loadFolder()]);
  }

  async function unload(model: LoadedModel): Promise<void> {
    if (unloading) return;
    unloading = model.modelPath;
    const name = model.modelId ?? model.model;
    try {
      await unloadLoadedModel(model.modelPath);
      addToast(get(t)("ai.loaded.unloaded_toast", { values: { name } }), "success");
    } catch (err: unknown) {
      const key = unloadErrorKey(err instanceof Error ? err.message : String(err));
      if (key) addToast(get(t)(key, { values: { name } }), "error");
      else reportError(err, { surface: "toast" });
    } finally {
      unloading = null;
      await loadLoaded();
    }
  }

  async function copy(text: string, okKey: string): Promise<void> {
    try {
      await navigator.clipboard.writeText(text);
      addToast(get(t)(okKey), "success");
    } catch {
      addToast(get(t)("ai.toast.copy_failed"), "error");
    }
  }

  async function run(action: () => Promise<void>): Promise<void> {
    if (working) return;
    working = true;
    try {
      await action();
    } catch (err: unknown) {
      reportError(err, { surface: "toast" });
    } finally {
      working = false;
    }
  }

  function onselect(model: InstalledModel, entry: MenuEntryId): void {
    const tr = get(t);
    switch (entry) {
      case "reveal":
        void run(() => revealInstalledModel(model.path));
        break;
      case "pin":
        void run(async () => {
          await setInstalledModelPinned(model.id, model.pinned_at === null);
          await loadInstalled();
        });
        break;
      case "copy_id":
        void copy(model.id, "ai.toast.copied_id");
        break;
      case "copy_path":
        void copy(model.path, "ai.toast.copied_path");
        break;
      case "source":
        if (model.source_url) void openExternalUrl(model.source_url);
        break;
      case "use_default":
        void run(async () => {
          await setDefaultModel(model.id);
          await reloadLlmFromDb();
          addToast(tr("ai.toast.default_set", { values: { name: model.id } }), "success");
          await loadInstalled();
        });
        break;
      case "use_search":
        searchTarget = model;
        break;
      case "delete":
        deleteTarget = model;
        break;
    }
  }

  function confirmSearch(): void {
    const model = searchTarget;
    if (!model?.embedding_entry) return;
    const entry = model.embedding_entry;
    void run(async () => {
      await setEmbeddingsDefault(entry);
      addToast(get(t)("ai.toast.search_set", { values: { name: model.id } }), "success");
      searchTarget = null;
      await loadInstalled();
    });
  }

  function confirmDelete(): void {
    const model = deleteTarget;
    if (!model) return;
    void run(async () => {
      try {
        await deleteInstalledModel(model.path);
        addToast(get(t)("ai.toast.deleted", { values: { name: model.id } }), "success");
      } finally {
        deleteTarget = null;
        await loadInstalled();
      }
    });
  }

  async function importFile(): Promise<void> {
    if (importing) return;
    importing = true;
    try {
      const selected = await openFilePicker({
        multiple: false,
        filters: [{ name: get(t)("ai.installed.import_title"), extensions: ["gguf"] }],
        title: get(t)("ai.installed.import_title"),
        defaultPath: modelsDir ?? undefined,
      });
      if (!selected) return;
      const filePath = typeof selected === "string" ? selected : (selected as { path: string }).path;
      const dest = await importModelFile(filePath, ["gguf"]);
      const name = dest.split(/[\\/]/).pop() ?? dest;
      addToast(get(t)("ai.toast.imported", { values: { name } }), "success");
      await loadInstalled();
    } catch (err: unknown) {
      reportError(err, { surface: "toast" });
    } finally {
      importing = false;
    }
  }

  function discover(): void {
    openSettingsWindow("model-hub");
  }

  function revealFolder(): void {
    if (modelsDir) void run(() => revealInstalledModel(modelsDir as string));
  }

  function onKeydown(event: KeyboardEvent): void {
    if (!isFilterShortcut(event, platform)) return;
    const input = document.querySelector<HTMLInputElement>('[data-testid="ai-filter-input"]');
    if (!input) return;
    event.preventDefault();
    input.focus();
    input.select();
  }

  $effect(() => {
    window.addEventListener("keydown", onKeydown);
    return () => window.removeEventListener("keydown", onKeydown);
  });

  onMount(() => {
    void refresh();
  });
</script>

<div class="flex h-full min-h-0 w-full" data-testid="llm-page">
  <nav
    class="flex w-56 shrink-0 flex-col gap-0.5 border-r border-border p-3"
    aria-label={$t("ai.categories_label")}
    data-testid="ai-categories"
  >
    <p class="px-2 pb-2 text-xs text-muted-foreground">{$t("ai.column_title")}</p>
    {#each CATEGORY_FILTERS as entry (entry)}
      <button
        type="button"
        class={cn(
          "flex items-center justify-between rounded-md px-2 py-1.5 text-left text-sm transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60",
          filter === entry
            ? "bg-muted font-medium text-foreground"
            : "text-muted-foreground hover:bg-muted/60 hover:text-foreground",
        )}
        aria-current={filter === entry ? "page" : undefined}
        onclick={() => (filter = entry)}
        data-testid="ai-category-{entry}"
      >
        <span>{$t(categoryLabelKey(entry))}</span>
        <span class="text-xs tabular-nums text-muted-foreground" data-testid="ai-category-count-{entry}">
          {categoryCount(installed, entry)}
        </span>
      </button>
    {/each}
  </nav>

  <div class="flex min-h-0 min-w-0 flex-1 flex-col gap-6 overflow-auto p-6" data-testid="ai-content">
    <LoadedModelsSection
      view={loaded}
      error={loadedError}
      {unloading}
      {pins}
      {pinning}
      onunload={unload}
      onpin={pin}
      onretry={loadLoaded}
    />
    <InstalledModelsSection
      models={installed}
      {loadedPaths}
      {filter}
      bind:query
      {platform}
      {modelsDir}
      error={installedError}
      {importing}
      {onselect}
      onimport={importFile}
      ondiscover={discover}
      onrevealfolder={revealFolder}
      onretry={loadInstalled}
    />
  </div>
</div>

<ConfirmDialog
  open={searchTarget !== null}
  onclose={() => (searchTarget = null)}
  onconfirm={confirmSearch}
  title={$t("ai.search_dialog.title", { values: { name: searchTarget?.id ?? "" } })}
  message={$t("ai.search_dialog.message")}
  confirmLabel={$t("ai.search_dialog.confirm")}
  loading={working}
  data-testid="ai-search-dialog"
/>

<ConfirmDialog
  open={deleteTarget !== null}
  onclose={() => (deleteTarget = null)}
  onconfirm={confirmDelete}
  title={$t("ai.delete_dialog.title", { values: { name: deleteTarget?.id ?? "" } })}
  message={$t("ai.delete_dialog.message")}
  confirmLabel={$t("ai.delete_dialog.confirm")}
  loading={working}
  data-testid="ai-delete-dialog"
/>
