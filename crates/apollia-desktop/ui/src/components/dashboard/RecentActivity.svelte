<script lang="ts">
  import { t } from "svelte-i18n";
  import type { TaskSummary } from "$lib/types";
  import { currentRoute } from "$lib/stores/navigation";
  import { Badge } from "$lib/components/ui/badge";
  import { Activity, CheckCircle, XCircle, Clock, AlertTriangle, Ban } from "lucide-svelte";
  import SmartOutputPreview from "../common/SmartOutputPreview.svelte";

  interface Props {
    tasks: TaskSummary[];
  }

  let { tasks: recentTasks }: Props = $props();

  const STATUS_CONFIG: Record<
    string,
    { variant: "default" | "secondary" | "destructive" | "outline"; extraClass: string; icon: typeof Activity; labelKey: string }
  > = {
    working: {
      variant: "outline",
      extraClass: "animate-pulse border-blue-500/50 text-blue-600 dark:text-blue-400",
      icon: Activity,
      labelKey: "dashboard.status_working",
    },
    submitted: {
      variant: "secondary",
      extraClass: "",
      icon: Clock,
      labelKey: "dashboard.status_submitted",
    },
    completed: {
      variant: "default",
      extraClass: "bg-[var(--apollia-success)] text-white",
      icon: CheckCircle,
      labelKey: "dashboard.status_completed",
    },
    failed: {
      variant: "destructive",
      extraClass: "",
      icon: XCircle,
      labelKey: "dashboard.status_failed",
    },
    input_required: {
      variant: "outline",
      extraClass: "border-[var(--apollia-warning)] text-[var(--apollia-warning)]",
      icon: AlertTriangle,
      labelKey: "dashboard.status_approval",
    },
    canceled: {
      variant: "secondary",
      extraClass: "",
      icon: Ban,
      labelKey: "dashboard.status_canceled",
    },
  };

  function configFor(status: string) {
    return STATUS_CONFIG[status] ?? STATUS_CONFIG.submitted;
  }

  function relativeTime(iso: string): string {
    const diffMs = Date.now() - new Date(iso).getTime();
    const diffSec = Math.floor(diffMs / 1000);
    if (diffSec < 5) return $t('dashboard.just_now');
    if (diffSec < 60) return `${diffSec}s`;
    const diffMin = Math.floor(diffSec / 60);
    if (diffMin < 60) return `${diffMin}m`;
    const diffHour = Math.floor(diffMin / 60);
    if (diffHour < 24) return `${diffHour}h`;
    const diffDay = Math.floor(diffHour / 24);
    return `${diffDay}j`;
  }

  function formatDuration(ms: number | undefined): string {
    if (ms === undefined) return "";
    if (ms < 1000) return `${ms}ms`;
    return `${(ms / 1000).toFixed(1)}s`;
  }

  function navigateToTasks() {
    currentRoute.set("tasks");
  }
</script>

{#if recentTasks.length === 0}
  <p class="text-sm text-muted-foreground">{$t('dashboard.no_recent_activity')}</p>
{:else}
  <div class="space-y-1" data-testid="recent-activity-list">
    {#each recentTasks as task (task.id)}
      {@const cfg = configFor(task.status)}
      {@const IconComponent = cfg.icon}
      <button
        class="flex w-full items-center gap-3 rounded-lg border border-transparent px-3 py-2.5 text-left text-sm transition-all hover:border-border hover:bg-[rgba(52,53,245,0.04)] dark:hover:bg-[rgba(124,95,214,0.06)]"
        onclick={navigateToTasks}
        data-testid="recent-activity-item"
        data-task-id={task.id}
      >
        <!-- Status icon -->
        <div class="shrink-0">
          <IconComponent size={14} class={task.status === "working" ? "animate-spin text-blue-500" : task.status === "completed" ? "text-green-500" : task.status === "failed" ? "text-red-500" : "text-muted-foreground"} />
        </div>

        <!-- Content -->
        <div class="min-w-0 flex-1">
          <div class="flex items-center gap-2">
            <span class="text-xs font-medium">{task.agent_name}</span>
            <Badge variant={cfg.variant} class="shrink-0 whitespace-nowrap text-[9px] px-1.5 py-0 {cfg.extraClass}">
              {$t(cfg.labelKey)}
            </Badge>
          </div>
          {#if task.status === "working"}
            <p class="mt-0.5 text-[11px] text-blue-500/80 animate-pulse">{$t('dashboard.task_in_progress')}</p>
          {:else if task.output_text || task.input_preview}
            <p class="mt-0.5 truncate text-[11px] text-muted-foreground">
              <SmartOutputPreview output={task.output_text ?? task.input_preview ?? ""} maxLength={60} class="inline" />
            </p>
          {/if}
        </div>

        <!-- Time + duration -->
        <div class="shrink-0 text-right">
          <span class="text-[11px] text-muted-foreground">{relativeTime(task.created_at)}</span>
          {#if task.duration_ms}
            <p class="text-[10px] text-muted-foreground/60">{formatDuration(task.duration_ms)}</p>
          {/if}
        </div>
      </button>
    {/each}
  </div>
{/if}
