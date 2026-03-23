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
  import { addToast } from "$lib/components/ui/toast/store";
  import { TabBar } from "$lib/components/ui/tabs";
  import EmptyState from "../components/common/EmptyState.svelte";
  import ConfirmDialog from "$lib/components/ui/dialog/ConfirmDialog.svelte";

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

  let displayedRuns = $derived(
    activeTab === "active" ? $activePipelineRuns : $historicPipelineRuns,
  );

  function handleDetail(runId: string) {
    detailRunId = runId;
    detailOpen = true;
  }

  function handleCloseDetail() {
    detailOpen = false;
  }

  function handleRunCreated(runId: string, pipelineId: string) {
    addToast($t('pipelines.run_launched', { values: { runId: runId.slice(0, 8), pipelineId } }), "success");
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
      addToast(err instanceof Error ? err.message : String(err), "error");
    } finally {
      loadingDefs = false;
    }
  }

  function handlePipelineCreated(id: string) {
    addToast($t("pipelines.created_toast", { values: { id } }), "success");
    void loadDefinitions();
  }

  function handlePipelineUpdated(id: string) {
    addToast($t("pipelines.updated_toast", { values: { id } }), "success");
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
      addToast($t("pipelines.deleted_toast", { values: { id: deletePipelineId } }), "success");
      showDeleteConfirm = false;
      void loadDefinitions();
    } catch (err: unknown) {
      addToast($t("pipelines.delete_error", { values: { message: err instanceof Error ? err.message : String(err) } }), "error");
    } finally {
      deleting = false;
    }
  }
</script>

<div class="max-w-6xl space-y-6" data-testid="pipelines-page">
  <!-- Header -->
  <div class="flex items-center justify-between">
    <div>
      <h1 class="text-2xl font-semibold">{$t('pipelines.title')}</h1>
      <p class="text-xs text-muted-foreground" data-testid="pipelines-subtitle">{$t('pipelines.subtitle')}</p>
    </div>
    <div class="flex gap-2">
      {#if activeTab === "definitions"}
        <Button size="sm" onclick={() => (showCreateDialog = true)} data-testid="create-pipeline-btn">{$t('pipelines.new_pipeline')}</Button>
      {:else}
        <Button size="sm" onclick={() => (showNewRunDialog = true)}>{$t('pipelines.run_pipeline')}</Button>
      {/if}
    </div>
  </div>

  <!-- Tabs (AC-1) -->
  <TabBar
    items={[
      { key: "active", label: $t("pipelines.tab_active"), count: $activePipelineRuns.length > 0 ? $activePipelineRuns.length : undefined },
      { key: "history", label: $t("pipelines.tab_history"), count: $historicPipelineRuns.length > 0 ? $historicPipelineRuns.length : undefined },
      { key: "definitions", label: $t("pipelines.tab_definitions"), count: definitions.length > 0 ? definitions.length : undefined },
    ]}
    activeTab={activeTab}
    ontabchange={(key) => handleTabChange(key as PipelineTab)}
    testidPrefix="pipeline"
  />

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
<ConfirmDialog
  open={showDeleteConfirm}
  onclose={() => { showDeleteConfirm = false; }}
  onconfirm={handleDeleteConfirm}
  title={$t("pipelines.delete_confirm_title")}
  message={$t("pipelines.delete_confirm_message", { values: { id: deletePipelineId } })}
  confirmLabel={$t("pipelines.delete_confirm_yes")}
  cancelLabel={$t("common.cancel")}
  loading={deleting}
  data-testid="delete-pipeline-confirm"
/>
