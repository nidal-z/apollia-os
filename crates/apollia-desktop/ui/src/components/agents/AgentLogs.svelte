<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { fly } from "svelte/transition";
  import { t } from "svelte-i18n";
  import type { TaskSummary } from "$lib/types";
  import { Sheet } from "$lib/components/ui/sheet";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import { ScrollText, RefreshCw, CheckCircle, XCircle, Activity, Clock, AlertTriangle, Ban } from "lucide-svelte";

  interface Props {
    agentId: string;
    open: boolean;
    onclose: () => void;
  }

  let { agentId, open, onclose }: Props = $props();

  let taskList = $state<TaskSummary[]>([]);
  let loading = $state(false);
  let error = $state<string | null>(null);

  const STATUS_I18N: Record<string, string> = {
    working: "dashboard.status_working",
    submitted: "dashboard.status_submitted",
    completed: "dashboard.status_completed",
    failed: "dashboard.status_failed",
    input_required: "dashboard.status_approval",
    canceled: "dashboard.status_canceled",
  };

  const STATUS_CONFIG: Record<string, { variant: "success" | "destructive" | "warning" | "info" | "secondary"; icon: typeof Activity }> = {
    completed: { variant: "success", icon: CheckCircle },
    working: { variant: "info", icon: Activity },
    submitted: { variant: "secondary", icon: Clock },
    failed: { variant: "destructive", icon: XCircle },
    input_required: { variant: "warning", icon: AlertTriangle },
    canceled: { variant: "secondary", icon: Ban },
  };

  function shortId(id: string): string {
    return id.slice(0, 8);
  }

  function formatDuration(ms: number | undefined): string {
    if (ms === undefined || ms === null) return "-";
    if (ms < 1000) return `${ms}ms`;
    return `${(ms / 1000).toFixed(1)}s`;
  }

  function formatRelativeTime(isoDate: string): string {
    if (!isoDate) return "-";
    const now = Date.now();
    const then = new Date(isoDate).getTime();
    const diffSecs = Math.floor((now - then) / 1000);
    if (diffSecs < 60) return `${diffSecs}s`;
    const diffMins = Math.floor(diffSecs / 60);
    if (diffMins < 60) return `${diffMins}m`;
    const diffHours = Math.floor(diffMins / 60);
    if (diffHours < 24) return `${diffHours}h`;
    return `${Math.floor(diffHours / 24)}j`;
  }

  async function fetchTasks() {
    loading = true;
    error = null;
    try {
      const result: TaskSummary[] = await invoke("list_tasks", { filter: { agent_id: agentId } });
      taskList = result.slice(0, 30);
    } catch (err: unknown) {
      error = err instanceof Error ? err.message : String(err);
      taskList = [];
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    if (open && agentId) {
      void fetchTasks();
    }
  });
</script>

<Sheet {open} {onclose} class="w-full sm:max-w-[480px]">
  <div class="flex h-full flex-col" data-testid="agent-logs-sheet">

    <!-- Header card -->
    <div class="mx-4 mt-6 rounded-xl glass-card glass-border overflow-hidden">
      <div class="h-0.5 w-full bg-secondary"></div>
      <div class="px-4 py-3.5 flex items-center justify-between">
        <div class="flex items-center gap-3">
          <div class="flex h-8 w-8 items-center justify-center rounded-lg bg-secondary/10">
            <ScrollText size={15} class="text-secondary" />
          </div>
          <div>
            <h2 class="text-sm font-medium">{$t('agents.logs_title')}</h2>
            <p class="text-[11px] text-muted-foreground">{taskList.length} {$t('agents.tasks_label')}</p>
          </div>
        </div>
        <Button size="sm" variant="ghost" class="h-7 w-7 p-0" onclick={fetchTasks} disabled={loading} aria-label={$t('common.refresh')}>
          <RefreshCw size={13} class="text-muted-foreground {loading ? 'animate-spin' : ''}" />
        </Button>
      </div>
    </div>

    <!-- Task list -->
    <div class="flex-1 overflow-auto px-4 pt-3 pb-6">
      {#if loading && taskList.length === 0}
        <div class="flex items-center justify-center py-12">
          <RefreshCw size={16} class="animate-spin text-muted-foreground" />
        </div>
      {:else if error}
        <div class="glass-card glass-border rounded-lg px-4 py-3">
          <p class="text-xs text-destructive">{error}</p>
        </div>
      {:else if taskList.length === 0}
        <div class="glass-card glass-border rounded-lg px-4 py-8 text-center">
          <ScrollText size={24} class="mx-auto text-muted-foreground/30 mb-2" />
          <p class="text-xs text-muted-foreground">{$t('agents.no_tasks')}</p>
        </div>
      {:else}
        <div class="glass-card glass-border rounded-lg overflow-hidden divide-y divide-border/40">
          {#each taskList as task, i (task.id)}
            {@const cfg = STATUS_CONFIG[task.status] ?? STATUS_CONFIG.submitted}
            {@const IconComponent = cfg.icon}
            <div
              class="flex items-center gap-3 px-3.5 py-2.5 transition-colors duration-150 hover:bg-primary/5"
              in:fly={{ y: 4, duration: 150, delay: i * 20 }}
            >
              <!-- Status icon -->
              <IconComponent
                size={13}
                class="{task.status === 'completed' ? 'text-success' :
                  task.status === 'failed' ? 'text-destructive' :
                  task.status === 'working' ? 'text-primary animate-spin' :
                  'text-muted-foreground'} shrink-0"
              />

              <!-- Task ID -->
              <code class="text-[10px] text-muted-foreground/50 font-mono shrink-0 w-14">{shortId(task.id)}</code>

              <!-- Status badge -->
              <Badge variant={cfg.variant} class="text-[9px] px-1.5 py-0 shrink-0">{$t(STATUS_I18N[task.status] ?? "dashboard.status_submitted")}</Badge>

              <!-- Output preview -->
              {#if task.output_text || task.input_preview}
                <span class="flex-1 truncate text-[11px] text-muted-foreground/50">
                  {(task.output_text ?? task.input_preview ?? "").slice(0, 40)}
                </span>
              {:else}
                <span class="flex-1"></span>
              {/if}

              <!-- Duration + time -->
              <div class="shrink-0 text-right">
                <span class="text-[10px] text-muted-foreground/40">{formatDuration(task.duration_ms)}</span>
                <span class="ml-1.5 text-[10px] text-muted-foreground/30">{formatRelativeTime(task.created_at)}</span>
              </div>
            </div>
          {/each}
        </div>
      {/if}
    </div>
  </div>
</Sheet>
