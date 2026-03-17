<script lang="ts">
  import { fly } from "svelte/transition";
  import { flip } from "svelte/animate";
  import { t } from "svelte-i18n";
  import { activePipelineRuns, historicPipelineRuns, pipelineRuns } from "$lib/stores/pipelines";
  import { Button } from "$lib/components/ui/button";
  import { GitBranch } from "lucide-svelte";
  import PipelineRunCard from "../components/pipelines/PipelineRunCard.svelte";
  import PipelineRunDetail from "../components/pipelines/PipelineRunDetail.svelte";
  import NewPipelineDialog from "../components/pipelines/NewPipelineDialog.svelte";
  import EmptyState from "../components/common/EmptyState.svelte";

  type PipelineTab = "active" | "history";

  let activeTab = $state<PipelineTab>("active");
  let detailRunId = $state<string | null>(null);
  let detailOpen = $state(false);
  let showNewRunDialog = $state(false);

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
  }
</script>

<div class="space-y-6">
  <!-- Header -->
  <div class="flex items-center justify-between">
    <div class="space-y-1">
      <h1 class="text-2xl font-bold">{$t('pipelines.title')}</h1>
      <p class="text-sm text-muted-foreground" data-testid="pipelines-subtitle">{$t('pipelines.subtitle')}</p>
    </div>
    <Button size="sm" onclick={() => (showNewRunDialog = true)}>{$t('pipelines.run_pipeline')}</Button>
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
  <div class="flex gap-1 rounded-md border bg-muted/30 p-1">
    <button
      class="rounded px-3 py-1 text-sm font-medium transition-colors {activeTab === 'active'
        ? 'bg-background text-foreground shadow-sm'
        : 'text-muted-foreground hover:text-foreground'}"
      onclick={() => handleTabChange("active")}
    >
      {$t('pipelines.tab_active')}
      {#if $activePipelineRuns.length > 0}
        <span class="ml-1 text-xs text-muted-foreground">({$activePipelineRuns.length})</span>
      {/if}
    </button>
    <button
      class="rounded px-3 py-1 text-sm font-medium transition-colors {activeTab === 'history'
        ? 'bg-background text-foreground shadow-sm'
        : 'text-muted-foreground hover:text-foreground'}"
      onclick={() => handleTabChange("history")}
    >
      {$t('pipelines.tab_history')}
      {#if $historicPipelineRuns.length > 0}
        <span class="ml-1 text-xs text-muted-foreground">({$historicPipelineRuns.length})</span>
      {/if}
    </button>
  </div>

  <!-- Run list or empty state -->
  {#if $pipelineRuns.length === 0}
    <EmptyState
      icon={GitBranch}
      title={$t('pipelines.empty_title')}
      subtitle={$t('pipelines.empty_subtitle')}
      ctaLabel={$t('pipelines.run_pipeline')}
      ctaAction={() => (showNewRunDialog = true)}
    />
  {:else if displayedRuns.length === 0}
    <div
      class="flex flex-col items-center justify-center gap-2 rounded-lg border border-dashed py-12"
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
