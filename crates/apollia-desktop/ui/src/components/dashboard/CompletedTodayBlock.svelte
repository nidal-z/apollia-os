<script lang="ts">
  import { t } from "svelte-i18n";
  import type { TaskSummary } from "$lib/types";
  import { navigateTo } from "$lib/stores/navigation";
  import { Activity, CheckCircle, XCircle, Clock, AlertTriangle, Ban } from "lucide-svelte";

  type Range = "today" | "week" | "all";

  interface Props {
    tasks: TaskSummary[];
    limit?: number;
  }

  let { tasks, limit = 10 }: Props = $props();

  let range = $state<Range>("today");

  const STATUS_ICON: Record<string, { icon: typeof Activity; color: string }> = {
    working: { icon: Activity, color: "text-primary" },
    submitted: { icon: Clock, color: "text-muted-foreground" },
    completed: { icon: CheckCircle, color: "text-success" },
    failed: { icon: XCircle, color: "text-destructive" },
    input_required: { icon: AlertTriangle, color: "text-warning" },
    canceled: { icon: Ban, color: "text-muted-foreground" },
  };

  function rangeStartIso(r: Range): string | null {
    if (r === "all") return null;
    const d = new Date();
    if (r === "today") {
      d.setHours(0, 0, 0, 0);
    } else {
      // week: rolling 7 days
      d.setDate(d.getDate() - 7);
    }
    return d.toISOString();
  }

  const filtered = $derived.by(() => {
    const floor = rangeStartIso(range);
    return tasks
      .filter((t) => t.status === "completed" || t.status === "failed" || t.status === "canceled")
      .filter((t) => !floor || t.created_at >= floor)
      .sort((a, b) => b.created_at.localeCompare(a.created_at));
  });

  const visible = $derived(filtered.slice(0, limit));
  const hasMore = $derived(filtered.length > limit);

  function relativeTime(iso: string): string {
    const diffMs = Date.now() - new Date(iso).getTime();
    const diffSec = Math.floor(diffMs / 1000);
    if (diffSec < 5) return $t("dashboard.just_now");
    if (diffSec < 60) return `${diffSec}s`;
    const diffMin = Math.floor(diffSec / 60);
    if (diffMin < 60) return `${diffMin}m`;
    const diffHour = Math.floor(diffMin / 60);
    if (diffHour < 24) return `${diffHour}h`;
    return `${Math.floor(diffHour / 24)}j`;
  }
</script>

<section
  class="glass-card glass-border rounded-lg p-4"
  data-testid="completed-today-block"
>
  <header class="mb-3 flex items-center justify-between gap-3">
    <h2 class="text-sm font-medium uppercase tracking-wider text-muted-foreground">
      {$t("dashboard.completed_today_title")}
    </h2>
    <div class="inline-flex items-center gap-0.5 rounded-md border border-border/50 bg-card p-0.5 text-[10px]" role="tablist">
      {#each ["today", "week", "all"] as r (r)}
        <button
          role="tab"
          aria-selected={range === r}
          class="rounded px-2 py-0.5 transition-colors {range === r ? 'bg-primary/10 text-primary' : 'text-muted-foreground hover:text-foreground'}"
          onclick={() => (range = r as Range)}
          data-testid="completed-range-{r}"
        >
          {$t(`dashboard.range_${r}`)}
        </button>
      {/each}
    </div>
  </header>

  {#if visible.length === 0}
    <p class="py-6 text-center text-xs text-muted-foreground">
      {$t("dashboard.completed_today_empty")}
    </p>
  {:else}
    <ul class="divide-y divide-border/40" data-testid="completed-today-list">
      {#each visible as task (task.id)}
        {@const cfg = STATUS_ICON[task.status] ?? STATUS_ICON.completed}
        {@const Icon = cfg.icon}
        <li class="flex items-center gap-2 py-2">
          <Icon size={13} class="{cfg.color} shrink-0" />
          <span class="shrink-0 w-24 truncate text-[12px] font-medium">{task.agent_name}</span>
          <span class="flex-1 truncate text-[11px] text-muted-foreground/80">
            {task.output_text?.slice(0, 60) ?? task.input_preview.slice(0, 60)}
          </span>
          <span class="shrink-0 text-[10px] text-muted-foreground/60">{relativeTime(task.created_at)}</span>
        </li>
      {/each}
    </ul>
    {#if hasMore}
      <button
        class="mt-3 w-full rounded-md border border-border/60 bg-card px-3 py-1.5 text-xs text-primary hover:bg-muted/40"
        onclick={() => navigateTo("tasks")}
        data-testid="completed-see-history"
      >
        {$t("dashboard.completed_see_history")}
      </button>
    {/if}
  {/if}
</section>
