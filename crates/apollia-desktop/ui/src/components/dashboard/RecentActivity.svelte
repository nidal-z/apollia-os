<script lang="ts">
  import { t } from "svelte-i18n";
  import type { TaskSummary } from "$lib/types";
  import { currentRoute } from "$lib/stores/navigation";
  import { Badge } from "$lib/components/ui/badge";

  interface Props {
    tasks: TaskSummary[];
  }

  let { tasks: recentTasks }: Props = $props();

  const MAX_PREVIEW_LENGTH = 80;

  const STATUS_BADGE: Record<
    string,
    { variant: "default" | "secondary" | "destructive" | "outline"; extraClass: string }
  > = {
    completed: { variant: "default", extraClass: "bg-[var(--apollia-success)] text-white" },
    failed: { variant: "destructive", extraClass: "" },
    working: { variant: "outline", extraClass: "animate-pulse border-blue-500 text-blue-500" },
    input_required: { variant: "outline", extraClass: "border-[var(--apollia-warning)] text-[var(--apollia-warning)]" },
    submitted: { variant: "secondary", extraClass: "" },
    canceled: { variant: "secondary", extraClass: "" },
  };

  function badgeFor(status: string) {
    return STATUS_BADGE[status] ?? { variant: "secondary" as const, extraClass: "" };
  }

  /** Truncate output text to a single-line preview. */
  function preview(task: TaskSummary): string {
    const text = task.output_text ?? task.input_preview;
    if (!text) return "";
    const firstLine = text.split("\n")[0];
    if (firstLine.length <= MAX_PREVIEW_LENGTH) return firstLine;
    return firstLine.slice(0, MAX_PREVIEW_LENGTH) + "…";
  }

  /** Format a timestamp to a relative string (e.g. "2m ago", "1h ago"). */
  function relativeTime(iso: string): string {
    const diffMs = Date.now() - new Date(iso).getTime();
    const diffSec = Math.floor(diffMs / 1000);
    if (diffSec < 60) return `${diffSec}s ago`;
    const diffMin = Math.floor(diffSec / 60);
    if (diffMin < 60) return `${diffMin}m ago`;
    const diffHour = Math.floor(diffMin / 60);
    if (diffHour < 24) return `${diffHour}h ago`;
    const diffDay = Math.floor(diffHour / 24);
    return `${diffDay}d ago`;
  }

  function navigateToTasks() {
    currentRoute.set("tasks");
  }
</script>

{#if recentTasks.length === 0}
  <p class="text-sm text-muted-foreground">{$t('dashboard.no_recent_activity')}</p>
{:else}
  <div class="space-y-2" data-testid="recent-activity-list">
    {#each recentTasks as task (task.id)}
      <button
        class="flex w-full items-center gap-3 rounded-md px-3 py-2 text-left text-sm transition-colors hover:bg-accent/30"
        onclick={navigateToTasks}
        data-testid="recent-activity-item"
        data-task-id={task.id}
      >
        <span class="min-w-0 flex-1">
          <span class="font-medium">{task.agent_name}</span>
          {#if preview(task)}
            <span class="text-muted-foreground"> — {preview(task)}</span>
          {/if}
        </span>
        <Badge variant={badgeFor(task.status).variant} class="{badgeFor(task.status).extraClass} shrink-0 text-[10px]">
          {$t(`common.status.${task.status === "completed" ? "active" : task.status === "failed" ? "error" : task.status}`) ?? task.status}
        </Badge>
        <span class="shrink-0 text-xs text-muted-foreground">{relativeTime(task.created_at)}</span>
      </button>
    {/each}
  </div>
{/if}
