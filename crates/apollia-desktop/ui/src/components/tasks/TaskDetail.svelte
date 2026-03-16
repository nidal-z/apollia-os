<script lang="ts">
  import { t } from "svelte-i18n";
  import type { TaskSummary } from "$lib/types";
  import { tasks } from "$lib/stores/tasks";
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
    <!-- Header -->
    <div class="flex items-center justify-between px-4 py-3">
      <div class="flex items-center gap-2">
        <h2 class="text-lg font-semibold" data-testid="task-detail-title">{$t('tasks.detail_title')}</h2>
        {#if task}
          <Badge
            variant={STATUS_VARIANT[task.status] ?? "secondary"}
            class={STATUS_EXTRA_CLASS[task.status] ?? ""}
          >
            {task.status.toUpperCase()}
          </Badge>
        {/if}
      </div>
      <Button size="sm" variant="ghost" onclick={onclose}>{$t('common.close')}</Button>
    </div>

    <Separator />

    <div class="flex-1 overflow-auto p-4">
      {#if !task}
        <p class="text-sm text-muted-foreground">{$t('tasks.not_found')}</p>
      {:else}
        <div class="space-y-4">
          <!-- Metadata -->
          <div class="space-y-1 text-sm">
            <div class="flex items-center gap-2">
              <span class="text-muted-foreground">{$t('tasks.id_label')}:</span>
              <code class="text-xs">{task.id}</code>
            </div>
            <div class="flex items-center gap-2">
              <span class="text-muted-foreground">{$t('tasks.agent')}:</span>
              <span>{task.agent_name || task.agent_id}</span>
            </div>
            <div class="flex items-center gap-2">
              <span class="text-muted-foreground">{$t('tasks.duration')}:</span>
              <span>{formatDuration(task.duration_ms)}</span>
            </div>
            <div class="flex items-center gap-2">
              <span class="text-muted-foreground">{$t('tasks.created')}:</span>
              <span>{formatDate(task.created_at)}</span>
            </div>
          </div>

          <Separator />

          <!-- Input section -->
          <div>
            <h3 class="mb-1 text-sm font-semibold">
              {$t('tasks.input')}
              {#if inputTruncated}
                <Badge variant="outline" class="ml-2 text-[10px]">[TRONQUE]</Badge>
              {/if}
            </h3>
            <div class="rounded border bg-muted/30 p-3">
              <p class="whitespace-pre-wrap text-sm">{task.input_preview || $t('common.no_input')}</p>
            </div>
          </div>

          {#if task.output_text}
            <Separator />

            <!-- Output section — smart formatted view (STORY-164) -->
            <div>
              <h3 class="mb-1 text-sm font-semibold">
                {$t('tasks.output')}
                {#if outputTruncated}
                  <Badge variant="outline" class="ml-2 text-[10px]">[TRONQUE]</Badge>
                {/if}
              </h3>
              <SmartOutput output={task.output_text} />
            </div>
          {/if}

          <Separator />

          <!-- Timeline section -->
          <div data-testid="task-timeline-section">
            <h3 class="mb-2 text-sm font-semibold">{$t('tasks.timeline')}</h3>
            <TaskTimeline taskId={taskId} isRunning={isRunning} />
          </div>
        </div>
      {/if}
    </div>
  </div>
</Sheet>
