<script lang="ts">
  import { t } from "svelte-i18n";
  import { fly } from "svelte/transition";
  import type { TaskSummary } from "$lib/types";
  import { currentRoute } from "$lib/stores/navigation";
  import { Badge } from "$lib/components/ui/badge";
  import { Activity, CheckCircle, XCircle, Clock, AlertTriangle, Ban } from "lucide-svelte";

  interface Props {
    tasks: TaskSummary[];
    onagentclick?: (agentId: string) => void;
  }

  let { tasks: recentTasks, onagentclick }: Props = $props();

  const STATUS_ICON: Record<string, { icon: typeof Activity; color: string }> = {
    working: { icon: Activity, color: "text-primary" },
    submitted: { icon: Clock, color: "text-muted-foreground" },
    completed: { icon: CheckCircle, color: "text-success" },
    failed: { icon: XCircle, color: "text-destructive" },
    input_required: { icon: AlertTriangle, color: "text-warning" },
    canceled: { icon: Ban, color: "text-muted-foreground" },
  };

  const STATUS_BADGE: Record<string, { variant: "success" | "destructive" | "warning" | "info" | "secondary"; labelKey: string }> = {
    working: { variant: "info", labelKey: "dashboard.status_working" },
    submitted: { variant: "secondary", labelKey: "dashboard.status_submitted" },
    completed: { variant: "success", labelKey: "dashboard.status_completed" },
    failed: { variant: "destructive", labelKey: "dashboard.status_failed" },
    input_required: { variant: "warning", labelKey: "dashboard.status_approval" },
    canceled: { variant: "secondary", labelKey: "dashboard.status_canceled" },
  };

  function relativeTime(iso: string): string {
    const diffMs = Date.now() - new Date(iso).getTime();
    const diffSec = Math.floor(diffMs / 1000);
    if (diffSec < 5) return $t('dashboard.just_now');
    if (diffSec < 60) return `${diffSec}s`;
    const diffMin = Math.floor(diffSec / 60);
    if (diffMin < 60) return `${diffMin}m`;
    const diffHour = Math.floor(diffMin / 60);
    if (diffHour < 24) return `${diffHour}h`;
    return `${Math.floor(diffHour / 24)}j`;
  }

  function formatDuration(ms: number | undefined): string {
    if (ms === undefined) return "";
    if (ms < 1000) return `${ms}ms`;
    return `${(ms / 1000).toFixed(1)}s`;
  }

  function handleTaskClick(task: TaskSummary) {
    if (task.agent_id && onagentclick) {
      onagentclick(task.agent_id);
    } else {
      currentRoute.set("tasks");
    }
  }
</script>

{#if recentTasks.length === 0}
  <div class="glass-card glass-border rounded-lg px-4 py-8 text-center">
    <p class="text-xs text-muted-foreground">{$t('dashboard.no_recent_activity')}</p>
  </div>
{:else}
  <div class="glass-card glass-border rounded-lg overflow-hidden divide-y divide-border/50" data-testid="recent-activity-list">
    {#each recentTasks as task, i (task.id)}
      {@const iconCfg = STATUS_ICON[task.status] ?? STATUS_ICON.submitted}
      {@const badgeCfg = STATUS_BADGE[task.status] ?? STATUS_BADGE.submitted}
      {@const IconComponent = iconCfg.icon}
      <button
        class="group flex w-full items-center gap-3 px-3.5 py-2.5 text-left transition-colors duration-150
          hover:bg-primary/5"
        onclick={() => handleTaskClick(task)}
        data-testid="recent-activity-item"
        data-task-id={task.id}
        in:fly={{ y: 4, duration: 150, delay: i * 30 }}
      >
        <!-- Icon -->
        <IconComponent size={13} class="{iconCfg.color} shrink-0 {task.status === 'working' ? 'animate-spin' : ''}" />

        <!-- Agent name -->
        <span class="shrink-0 text-xs font-medium w-28 truncate">{task.agent_name}</span>

        <!-- Badge -->
        <Badge variant={badgeCfg.variant} class="shrink-0 text-[9px] px-1.5 py-0">
          {$t(badgeCfg.labelKey)}
        </Badge>

        <!-- Output preview -->
        <span class="flex-1 truncate text-[11px] text-muted-foreground/60">
          {#if task.output_text}
            {task.output_text.slice(0, 50)}
          {:else if task.input_preview}
            {task.input_preview.slice(0, 50)}
          {/if}
        </span>

        <!-- Time -->
        <div class="shrink-0 text-right">
          <span class="text-[10px] text-muted-foreground/50">{relativeTime(task.created_at)}</span>
          {#if task.duration_ms}
            <span class="ml-1 text-[10px] text-muted-foreground/30">{formatDuration(task.duration_ms)}</span>
          {/if}
        </div>
      </button>
    {/each}
  </div>
{/if}
