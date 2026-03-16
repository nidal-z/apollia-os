<!--
  Task detail drawer — restructured with business result first (STORY-166).

  Layout order:
  1. Header: agent name + status badge + duration + timestamp (no UUID)
  2. Input section (collapsed if > 3 lines)
  3. Result section (main area, uses SmartOutput)
  4. Technical details (expandable, closed by default in operator mode)
-->
<script lang="ts">
  import { t } from "svelte-i18n";
  import type { TaskSummary } from "$lib/types";
  import { tasks } from "$lib/stores/tasks";
  import { uiMode } from "$lib/stores/mode";
  import { Sheet } from "$lib/components/ui/sheet";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import { Separator } from "$lib/components/ui/separator";
  import TaskTimeline from "./TaskTimeline.svelte";
  import SmartOutput from "../common/SmartOutput.svelte";

  interface Props {
    taskId: string;
    open: boolean;
    onclose: () => void;
  }

  let { taskId, open, onclose }: Props = $props();

  const TRUNCATION_MARKER = "[TRONQUE]";
  const INPUT_COLLAPSE_LINE_COUNT = 3;

  const STATUS_VARIANT: Record<string, "default" | "secondary" | "destructive" | "outline"> = {
    completed: "default",
    working: "outline",
    submitted: "secondary",
    failed: "destructive",
    input_required: "outline",
    canceled: "secondary",
  };

  const STATUS_EXTRA_CLASS: Record<string, string> = {
    completed: "bg-[var(--apollia-success)] text-white",
    working: "animate-pulse border-blue-500 text-blue-500",
    failed: "",
    input_required: "border-[var(--apollia-warning)] text-[var(--apollia-warning)]",
  };

  let task = $derived<TaskSummary | undefined>(
    $tasks.find((t) => t.id === taskId),
  );

  let isRunning = $derived(
    task?.status === "working" || task?.status === "submitted",
  );

  let inputTruncated = $derived(
    task?.input_preview?.includes(TRUNCATION_MARKER) ?? false,
  );

  let outputTruncated = $derived(
    task?.output_text?.includes(TRUNCATION_MARKER) ?? false,
  );

  let inputNeedsCollapse = $derived(
    (task?.input_preview?.split("\n").length ?? 0) > INPUT_COLLAPSE_LINE_COUNT,
  );

  let inputExpanded = $state(false);

  let mode = $derived($uiMode);

  function formatDuration(ms: number | undefined): string {
    if (ms === undefined || ms === null) return "-";
    if (ms < 1000) return `${ms}ms`;
    if (ms < 60_000) return `${(ms / 1000).toFixed(1)}s`;
    const mins = Math.floor(ms / 60_000);
    const secs = Math.floor((ms % 60_000) / 1000);
    return `${mins}m ${secs}s`;
  }

  function formatDate(iso: string): string {
    if (!iso) return "-";
    return new Date(iso).toLocaleString();
  }
</script>

<Sheet open={open} onclose={onclose} class="w-[500px]">
  <div class="flex h-full flex-col" data-testid="task-detail">
    {#if !task}
      <div class="flex-1 p-4">
        <p class="text-sm text-muted-foreground">{$t('tasks.not_found')}</p>
      </div>
    {:else}
      <!-- 1. Header: agent name + badge + duration + timestamp -->
      <div class="flex items-center justify-between px-4 py-3" data-testid="task-detail-header">
        <div class="flex items-center gap-2 min-w-0">
          <h2 class="truncate text-lg font-semibold" data-testid="task-detail-title">
            {task.agent_name || task.agent_id}
          </h2>
          <Badge
            variant={STATUS_VARIANT[task.status] ?? "secondary"}
            class={STATUS_EXTRA_CLASS[task.status] ?? ""}
          >
            {task.status.toUpperCase()}
          </Badge>
          <span class="shrink-0 text-xs text-muted-foreground">
            {formatDuration(task.duration_ms)}
          </span>
          <span class="shrink-0 text-xs text-muted-foreground">
            {formatDate(task.created_at)}
          </span>
        </div>
        <Button size="sm" variant="ghost" onclick={onclose} class="shrink-0 ml-2">
          {$t('common.close')}
        </Button>
      </div>

      <Separator />

      <div class="flex-1 overflow-auto p-4">
        <div class="space-y-4">
          <!-- 2. Input section (collapse if > 3 lines) -->
          <section data-testid="task-detail-input">
            <h3 class="mb-1 text-sm font-semibold">
              {$t('tasks.input')}
              {#if inputTruncated}
                <Badge variant="outline" class="ml-2 text-[10px]">[TRONQUE]</Badge>
              {/if}
            </h3>
            <div class="rounded border bg-muted/30 p-3">
              {#if inputNeedsCollapse && !inputExpanded}
                <p class="line-clamp-3 whitespace-pre-wrap text-sm">
                  {task.input_preview || $t('common.no_input')}
                </p>
                <button
                  class="mt-1 text-xs text-muted-foreground underline hover:text-foreground"
                  onclick={() => inputExpanded = true}
                >
                  {$t('tasks.show_details')}
                </button>
              {:else}
                <p class="whitespace-pre-wrap text-sm">
                  {task.input_preview || $t('common.no_input')}
                </p>
                {#if inputNeedsCollapse}
                  <button
                    class="mt-1 text-xs text-muted-foreground underline hover:text-foreground"
                    onclick={() => inputExpanded = false}
                  >
                    {$t('tasks.hide_details')}
                  </button>
                {/if}
              {/if}
            </div>
          </section>

          <!-- 3. Result section (main area — largest visual section) -->
          {#if task.output_text}
            <Separator />

            <section class="flex-1" data-testid="task-detail-result">
              <h3 class="mb-1 text-sm font-semibold">
                {$t('tasks.result')}
                {#if outputTruncated}
                  <Badge variant="outline" class="ml-2 text-[10px]">[TRONQUE]</Badge>
                {/if}
              </h3>
              <SmartOutput output={task.output_text} />
            </section>
          {/if}

          <Separator />

          <!-- 4. Technical details (expandable — closed in operator, open in builder) -->
          <details open={mode === "builder"} data-testid="task-detail-technical">
            <summary class="cursor-pointer text-xs text-muted-foreground hover:text-foreground">
              {$t('tasks.technical_details')}
            </summary>
            <div class="mt-2 space-y-3">
              <div class="space-y-1 text-sm">
                <div class="flex items-center gap-2">
                  <span class="text-muted-foreground">{$t('tasks.id_label')}:</span>
                  <code class="text-xs">{task.id}</code>
                </div>
              </div>

              <div data-testid="task-timeline-section">
                <h4 class="mb-2 text-sm font-semibold">{$t('tasks.timeline')}</h4>
                <TaskTimeline taskId={taskId} isRunning={isRunning} />
              </div>
            </div>
          </details>
        </div>
      </div>
    {/if}
  </div>
</Sheet>
