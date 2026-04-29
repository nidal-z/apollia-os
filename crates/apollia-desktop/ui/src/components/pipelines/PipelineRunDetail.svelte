<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import type { PipelineRunDetail as RunDetail } from "$lib/types";
  import { currentRoute } from "$lib/stores/navigation";
  import { Sheet } from "$lib/components/ui/sheet";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import { Separator } from "$lib/components/ui/separator";
  import { Skeleton } from "$lib/components/ui/skeleton";

  interface Props {
    runId: string;
    open: boolean;
    onclose: () => void;
  }

  let { runId, open, onclose }: Props = $props();

  let detail = $state<RunDetail | null>(null);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let expandedSteps = $state<Set<string>>(new Set());
  let pollTimer = $state<ReturnType<typeof setInterval> | null>(null);

  const STEP_STATUS_VARIANT: Record<string, "default" | "secondary" | "destructive" | "info" | "success" | "warning"> = {
    pending: "secondary",
    running: "info",
    completed: "success",
    failed: "destructive",
    waiting_approval: "warning",
  };

  const STEP_STATUS_ICON: Record<string, string> = {
    pending: "○",
    running: "◉",
    completed: "●",
    failed: "✕",
  };

  let isRunning = $derived(
    detail?.status === "running" || detail?.status === "waiting_approval",
  );

  let completedStepCount = $derived(
    detail ? detail.step_runs.filter((s) => s.status === "completed").length : 0,
  );

  let totalStepCount = $derived(detail ? detail.step_runs.length : 0);

  async function loadDetail(): Promise<void> {
    loading = true;
    error = null;
    try {
      detail = await invoke("get_pipeline_run_detail", { runId });
    } catch (err: unknown) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      loading = false;
    }
  }

  function toggleStep(stepId: string) {
    const next = new Set(expandedSteps);
    if (next.has(stepId)) {
      next.delete(stepId);
    } else {
      next.add(stepId);
    }
    expandedSteps = next;
  }

  function navigateToApprovals() {
    currentRoute.set("inbox");
    onclose();
  }

  function formatDuration(startedAt: string | null, endedAt: string | null): string {
    if (!startedAt) return "-";
    const start = new Date(startedAt).getTime();
    const end = endedAt ? new Date(endedAt).getTime() : Date.now();
    const ms = end - start;
    if (ms < 1000) return `${ms}ms`;
    if (ms < 60_000) return `${(ms / 1000).toFixed(1)}s`;
    const mins = Math.floor(ms / 60_000);
    const secs = Math.floor((ms % 60_000) / 1000);
    return `${mins}m ${secs}s`;
  }

  function formatDate(iso: string | null): string {
    if (!iso) return "-";
    return new Date(iso).toLocaleString();
  }

  function startPolling() {
    stopPolling();
    pollTimer = setInterval(() => {
      void loadDetail();
    }, 2000);
  }

  function stopPolling() {
    if (pollTimer !== null) {
      clearInterval(pollTimer);
      pollTimer = null;
    }
  }

  $effect(() => {
    if (open && runId) {
      void loadDetail();
    }
    return () => stopPolling();
  });

  $effect(() => {
    if (open && isRunning) {
      startPolling();
    } else {
      stopPolling();
    }
  });
</script>

<Sheet {open} onclose={onclose} class="w-full sm:max-w-[600px]">
  <div class="flex h-full flex-col">
    <!-- Header -->
    <div class="flex items-center justify-between px-4 py-3">
      <div class="flex items-center gap-2">
        <h2 class="text-lg font-medium">{$t('pipelines.detail_title')}</h2>
        {#if detail}
          <Badge
            variant={STEP_STATUS_VARIANT[detail.status] ?? "secondary"}
          >
            {detail.status.toUpperCase()}
          </Badge>
        {/if}
      </div>
      <Button size="sm" variant="ghost" onclick={onclose}>{$t('common.close')}</Button>
    </div>

    <Separator />

    <div class="flex-1 overflow-auto p-4">
      {#if loading && !detail}
        <div class="space-y-3">
          <Skeleton width="100%" height="2rem" />
          <Skeleton width="80%" height="1.5rem" />
          <Skeleton width="100%" height="4rem" />
        </div>
      {:else if error}
        <p class="text-sm text-destructive">{error}</p>
      {:else if !detail}
        <p class="text-sm text-muted-foreground">{$t('pipelines.not_found')}</p>
      {:else}
        <div class="space-y-4">
          <!-- Metadata -->
          <div class="space-y-1 text-sm">
            <div class="flex items-center gap-2">
              <span class="text-muted-foreground">{$t('pipelines.run_id')}:</span>
              <code class="text-xs">{detail.run_id}</code>
            </div>
            <div class="flex items-center gap-2">
              <span class="text-muted-foreground">{$t('pipelines.pipeline')}:</span>
              <span>{detail.pipeline_id}</span>
            </div>
            <div class="flex items-center gap-2">
              <span class="text-muted-foreground">{$t('pipelines.duration')}:</span>
              <span>{formatDuration(detail.started_at, detail.ended_at)}</span>
            </div>
            <div class="flex items-center gap-2">
              <span class="text-muted-foreground">{$t('pipelines.started')}:</span>
              <span>{formatDate(detail.started_at)}</span>
            </div>
            {#if detail.ended_at}
              <div class="flex items-center gap-2">
                <span class="text-muted-foreground">{$t('pipelines.ended')}:</span>
                <span>{formatDate(detail.ended_at)}</span>
              </div>
            {/if}
          </div>

          <Separator />

          <!-- Progress bar -->
          <div class="space-y-1">
            <div class="flex items-center justify-between text-xs text-muted-foreground">
              <span>{$t('pipelines.steps')}</span>
              <span>{completedStepCount} / {totalStepCount}</span>
            </div>
            <div class="h-2 w-full overflow-hidden rounded-full bg-muted">
              <div
                class="h-full rounded-full bg-success transition-all"
                style="width: {totalStepCount > 0 ? (completedStepCount / totalStepCount) * 100 : 0}%"
              ></div>
            </div>
          </div>

          <Separator />

          <!-- Steps timeline -->
          <div>
            <h3 class="mb-3 text-sm font-medium">{$t('pipelines.steps')}</h3>
            <div class="relative space-y-0">
              {#each detail.step_runs as step, i (step.step_id)}
                <div class="relative flex gap-3 pb-4">
                  <!-- Vertical line -->
                  {#if i < detail.step_runs.length - 1}
                    <div
                      class="absolute left-[7px] top-5 h-full w-px {step.status === 'completed'
                        ? 'bg-success'
                        : 'bg-border'}"
                    ></div>
                  {/if}

                  <!-- Status dot -->
                  <div class="relative z-10 flex h-4 w-4 shrink-0 items-center justify-center text-xs">
                    <span
                      class="{step.status === 'completed'
                        ? 'text-success'
                        : step.status === 'running'
                          ? 'text-primary animate-pulse'
                          : step.status === 'failed'
                            ? 'text-destructive'
                            : 'text-muted-foreground'}"
                    >
                      {STEP_STATUS_ICON[step.status] ?? "○"}
                    </span>
                  </div>

                  <!-- Step content -->
                  <div class="min-w-0 flex-1">
                    <div
                      class="flex w-full cursor-pointer items-center gap-2 text-left text-sm"
                      role="button"
                      tabindex="0"
                      onclick={() => toggleStep(step.step_id)}
                      onkeydown={(e) => e.key === "Enter" && toggleStep(step.step_id)}
                    >
                      <span class="font-medium">{step.step_id}</span>
                      <Badge
                        variant={STEP_STATUS_VARIANT[step.status] ?? "secondary"}
                        class="text-[10px]"
                      >
                        {step.status}
                      </Badge>
                      {#if step.started_at}
                        <span class="text-xs text-muted-foreground">
                          {formatDuration(step.started_at, step.ended_at)}
                        </span>
                      {/if}
                      {#if detail?.status === "waiting_approval"}
                        <button
                          class="ml-auto text-xs text-warning underline"
                          onclick={(e) => { e.stopPropagation(); navigateToApprovals(); }}
                        >
                          {$t('pipelines.view_approvals')}
                        </button>
                      {/if}
                    </div>

                    <!-- Expandable content -->
                    {#if expandedSteps.has(step.step_id)}
                      <div class="mt-2 space-y-2">
                        {#if step.output}
                          <div>
                            <p class="mb-1 text-xs font-medium text-muted-foreground">{$t('pipelines.step_output')}</p>
                            <div class="rounded glass-border glass-surface p-2">
                              <p class="whitespace-pre-wrap text-xs">{step.output}</p>
                            </div>
                          </div>
                        {/if}
                        {#if step.error}
                          <div>
                            <p class="mb-1 text-xs font-medium text-muted-foreground">{$t('pipelines.step_error')}</p>
                            <div class="rounded border border-destructive/30 bg-destructive/5 p-2">
                              <p class="whitespace-pre-wrap text-xs text-destructive">
                                {step.error}
                              </p>
                            </div>
                          </div>
                        {/if}
                        {#if !step.output && !step.error}
                          <p class="text-xs text-muted-foreground">{$t('pipelines.no_details')}</p>
                        {/if}
                      </div>
                    {/if}
                  </div>
                </div>
              {/each}
            </div>
          </div>
        </div>
      {/if}
    </div>
  </div>
</Sheet>
