<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { fly } from "svelte/transition";
  import { flip } from "svelte/animate";
  import { t } from "svelte-i18n";
  import { activePipelineRuns, historicPipelineRuns, pipelineRuns } from "$lib/stores/pipelines";
  import { Button } from "$lib/components/ui/button";
  import { GitBranch } from "lucide-svelte";
  import type { PipelineDefinitionView } from "$lib/types";
  import PipelineRunCard from "../components/pipelines/PipelineRunCard.svelte";
  import PipelineRunDetail from "../components/pipelines/PipelineRunDetail.svelte";
  import NewPipelineDialog from "../components/pipelines/NewPipelineDialog.svelte";
  import PipelineDefinitionCard from "../components/pipelines/PipelineDefinitionCard.svelte";
  import CreatePipelineDialog from "../components/pipelines/CreatePipelineDialog.svelte";
  import EditPipelineDialog from "../components/pipelines/EditPipelineDialog.svelte";
  import EmptyState from "../components/common/EmptyState.svelte";

  type PipelineTab = "active" | "history" | "definitions";

  let activeTab = $state<PipelineTab>("active");
  let detailRunId = $state<string | null>(null);
  let detailOpen = $state(false);
  let showNewRunDialog = $state(false);
  let showCreateDialog = $state(false);
  let showEditDialog = $state(false);
  let editPipelineId = $state("");
  let showDeleteConfirm = $state(false);
  let deletePipelineId = $state("");
  let deleting = $state(false);

  let definitions = $state<PipelineDefinitionView[]>([]);
  let loadingDefs = $state(false);

  let toast = $state<{ message: string; type: "success" | "error" } | null>(null);
  let toastTimer = $state<ReturnType<typeof setTimeout> | null>(null);

  let displayedRuns = $derived(
    activeTab === "active" ? $activePipelineRuns : $historicPipelineRuns,
  );

  function showToast(message: string, type: "success" | "error") {
    if (toastTimer !== null) {
      clearTimeout(toastTimer);
    }
    toast = { message, type };
    toastTimer = setTimeout(() => {
      toast = null;
      toastTimer = null;
    }, 4000);
  }

  function handleDetail(runId: string) {
    detailRunId = runId;
    detailOpen = true;
  }

  function handleCloseDetail() {
    detailOpen = false;
  }

  function handleRunCreated(runId: string, pipelineId: string) {
    showToast($t('pipelines.run_launched', { values: { runId: runId.slice(0, 8), pipelineId } }), "success");
    activeTab = "active";
    handleDetail(runId);
  }

  function handleTabChange(tab: PipelineTab) {
    activeTab = tab;
    if (tab === "definitions") {
      void loadDefinitions();
    }
  }

  async function loadDefinitions(): Promise<void> {
    loadingDefs = true;
    try {
      definitions = await invoke("list_pipeline_definitions");
    } catch (err: unknown) {
      definitions = [];
      showToast(err instanceof Error ? err.message : String(err), "error");
    } finally {
      loadingDefs = false;
    }
  }

  function handlePipelineCreated(id: string) {
    showToast($t("pipelines.created_toast", { values: { id } }), "success");
    void loadDefinitions();
  }

  function handlePipelineUpdated(id: string) {
    showToast($t("pipelines.updated_toast", { values: { id } }), "success");
    void loadDefinitions();
  }

  function handleEdit(id: string) {
    editPipelineId = id;
    showEditDialog = true;
  }

  function handleDeleteRequest(id: string) {
    deletePipelineId = id;
    showDeleteConfirm = true;
  }

  async function handleDeleteConfirm(): Promise<void> {
    deleting = true;
    try {
      await invoke("delete_pipeline", { id: deletePipelineId });
      showToast($t("pipelines.deleted_toast", { values: { id: deletePipelineId } }), "success");
      showDeleteConfirm = false;
      void loadDefinitions();
    } catch (err: unknown) {
      showToast($t("pipelines.delete_error", { values: { message: err instanceof Error ? err.message : String(err) } }), "error");
    } finally {
      deleting = false;
    }
  }
</script>

<div class="space-y-6">
  <!-- Header -->
  <div class="flex items-center justify-between">
    <div class="space-y-1">
      <h1 class="text-2xl font-bold">{$t('pipelines.title')}</h1>
      <p class="text-sm text-muted-foreground" data-testid="pipelines-subtitle">{$t('pipelines.subtitle')}</p>
    </div>
    <div class="flex gap-2">
      {#if activeTab === "definitions"}
        <Button size="sm" onclick={() => (showCreateDialog = true)} data-testid="create-pipeline-btn">{$t('pipelines.new_pipeline')}</Button>
      {:else}
        <Button size="sm" onclick={() => (showNewRunDialog = true)}>{$t('pipelines.run_pipeline')}</Button>
      {/if}
    </div>
  </div>

  <!-- Toast -->
  {#if toast}
    <div
      class="rounded-md border px-4 py-2 text-sm {toast.type === 'success'
        ? 'border-[var(--apollia-success)] bg-[var(--apollia-success)]/10 text-[var(--apollia-success)]'
        : 'border-[hsl(var(--destructive))] bg-[hsl(var(--destructive))]/10 text-[hsl(var(--destructive))]'}"
    >
      {toast.message}
    </div>
  {/if}

  <!-- Tabs -->
  <div class="flex gap-1 rounded-md glass-border glass-surface p-1">
    <button
      class="rounded px-3 py-1 text-sm font-medium transition-colors {activeTab === 'active'
        ? 'glass-inset text-foreground shadow-sm'
        : 'text-muted-foreground hover:text-foreground'}"
      onclick={() => handleTabChange("active")}
      data-testid="pipelines-tab-active"
    >
      {$t('pipelines.tab_active')}
      {#if $activePipelineRuns.length > 0}
        <span class="ml-1 text-xs text-muted-foreground">({$activePipelineRuns.length})</span>
      {/if}
    </button>
    <button
      class="rounded px-3 py-1 text-sm font-medium transition-colors {activeTab === 'history'
        ? 'glass-inset text-foreground shadow-sm'
        : 'text-muted-foreground hover:text-foreground'}"
      onclick={() => handleTabChange("history")}
      data-testid="pipelines-tab-history"
    >
      {$t('pipelines.tab_history')}
      {#if $historicPipelineRuns.length > 0}
        <span class="ml-1 text-xs text-muted-foreground">({$historicPipelineRuns.length})</span>
      {/if}
    </button>
    <button
      class="rounded px-3 py-1 text-sm font-medium transition-colors {activeTab === 'definitions'
        ? 'glass-inset text-foreground shadow-sm'
        : 'text-muted-foreground hover:text-foreground'}"
      onclick={() => handleTabChange("definitions")}
      data-testid="pipelines-tab-definitions"
    >
      {$t('pipelines.tab_definitions')}
      {#if definitions.length > 0}
        <span class="ml-1 text-xs text-muted-foreground">({definitions.length})</span>
      {/if}
    </button>
  </div>

  <!-- Content based on active tab -->
  {#if activeTab === "definitions"}
    <!-- Definitions tab content -->
    {#if loadingDefs}
      <p class="py-8 text-center text-sm text-muted-foreground">{$t("common.loading")}</p>
    {:else if definitions.length === 0}
      <div
        class="flex flex-col items-center justify-center gap-2 rounded-xl glass-surface glass-border border-dashed py-12"
      >
        <GitBranch class="h-8 w-8 text-muted-foreground" />
        <p class="text-muted-foreground">{$t('pipelines.no_definitions')}</p>
        <Button size="sm" variant="outline" onclick={() => (showCreateDialog = true)} data-testid="create-pipeline-empty-btn">
          {$t('pipelines.new_pipeline')}
        </Button>
      </div>
    {:else}
      <div class="space-y-2">
        {#each definitions as def (def.id)}
          <div animate:flip={{ duration: 300 }} in:fly={{ y: 10, duration: 200 }}>
            <PipelineDefinitionCard
              definition={def}
              onedit={handleEdit}
              ondelete={handleDeleteRequest}
            />
          </div>
        {/each}
      </div>
    {/if}
  {:else}
    <!-- Active / History tab content -->
    {#if $pipelineRuns.length === 0}
      <EmptyState
        icon={GitBranch}
        title={$t('pipelines.empty_title')}
        subtitle={$t('pipelines.empty_subtitle')}
        ctaLabel={$t('pipelines.run_pipeline')}
        ctaAction={() => (showNewRunDialog = true)}
        page="pipelines"
      />
    {:else if displayedRuns.length === 0}
      <div
        class="flex flex-col items-center justify-center gap-2 rounded-xl glass-surface glass-border border-dashed py-12"
      >
        <p class="text-muted-foreground">
          {activeTab === "active"
            ? $t('pipelines.no_active')
            : $t('pipelines.no_history')}
        </p>
      </div>
    {:else}
      <div class="space-y-2">
        {#each displayedRuns as run (run.run_id)}
          <div animate:flip={{ duration: 300 }} in:fly={{ y: 10, duration: 200 }}>
            <PipelineRunCard {run} ondetail={handleDetail} />
          </div>
        {/each}
      </div>
    {/if}
  {/if}
</div>

<!-- Detail sheet -->
{#if detailRunId}
  <PipelineRunDetail runId={detailRunId} open={detailOpen} onclose={handleCloseDetail} />
{/if}

<!-- New pipeline run dialog -->
<NewPipelineDialog
  open={showNewRunDialog}
  onclose={() => (showNewRunDialog = false)}
  onrun={handleRunCreated}
/>

<!-- Create pipeline definition dialog -->
<CreatePipelineDialog
  open={showCreateDialog}
  onclose={() => (showCreateDialog = false)}
  oncreated={handlePipelineCreated}
/>

<!-- Edit pipeline definition dialog -->
<EditPipelineDialog
  open={showEditDialog}
  pipelineId={editPipelineId}
  onclose={() => (showEditDialog = false)}
  onupdated={handlePipelineUpdated}
/>

<!-- Delete confirmation dialog -->
{#if showDeleteConfirm}
  <div
    class="fixed inset-0 z-50 flex items-center justify-center bg-black/50"
    role="button"
    tabindex="-1"
    onclick={() => (showDeleteConfirm = false)}
    onkeydown={(e) => { if (e.key === "Escape") showDeleteConfirm = false; }}
  >
    <div
      class="w-[400px] rounded-lg border bg-background p-6 shadow-lg"
      role="dialog"
      aria-modal="true"
      tabindex="-1"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => { if (e.key === "Escape") showDeleteConfirm = false; }}
      data-testid="delete-pipeline-confirm-dialog"
    >
      <h3 class="mb-2 text-lg font-semibold">{$t("pipelines.delete_confirm_title")}</h3>
      <p class="mb-4 text-sm text-muted-foreground">
        {$t("pipelines.delete_confirm_message", { values: { id: deletePipelineId } })}
      </p>
      <div class="flex justify-end gap-2">
        <Button variant="outline" size="sm" onclick={() => (showDeleteConfirm = false)} data-testid="delete-pipeline-cancel-btn">
          {$t("common.cancel")}
        </Button>
        <Button
          size="sm"
          variant="destructive"
          onclick={handleDeleteConfirm}
          disabled={deleting}
          data-testid="delete-pipeline-confirm-btn"
        >
          {deleting ? $t("pipelines.deleting") : $t("pipelines.delete_confirm_yes")}
        </Button>
      </div>
    </div>
  </div>
{/if}
