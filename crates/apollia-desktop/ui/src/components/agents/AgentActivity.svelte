<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import type { TaskSummary } from "$lib/types";
  import { Badge } from "$lib/components/ui/badge";
  import SmartOutputPreview from "../common/SmartOutputPreview.svelte";
  import { createIdentityGuard } from "$lib/utils/identityGuard";

  interface Props {
    agentId: string;
    onTaskClick: (taskId: string) => void;
  }

  let { agentId, onTaskClick }: Props = $props();

  const RECENT_LIMIT = 10;

  let taskList = $state<TaskSummary[]>([]);
  let loading = $state(false);
  let error = $state<string | null>(null);

  const TASK_STATUS_VARIANT: Record<string, "default" | "secondary" | "destructive" | "outline"> = {
    completed: "default",
    working: "outline",
    submitted: "secondary",
    failed: "destructive",
    input_required: "outline",
    canceled: "secondary",
  };

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
    if (diffSecs < 60) return `${diffSecs}s ago`;
    const diffMins = Math.floor(diffSecs / 60);
    if (diffMins < 60) return `${diffMins}m ago`;
    const diffHours = Math.floor(diffMins / 60);
    if (diffHours < 24) return `${diffHours}h ago`;
    const diffDays = Math.floor(diffHours / 24);
    return `${diffDays}d ago`;
  }

  // The panel instance survives a change of assistant, so a listing that comes
  // back late must not paint the previous assistant's tasks under the new name.
  const agentGuard = createIdentityGuard(() => agentId);

  async function fetchTasks() {
    const ticket = agentGuard.begin();
    loading = true;
    error = null;
    try {
      const result: TaskSummary[] = await invoke("list_tasks", {
        filter: { agent_id: agentId },
      });
      if (!ticket.current) return;
      taskList = result
        .sort((a, b) => b.created_at.localeCompare(a.created_at))
        .slice(0, RECENT_LIMIT);
    } catch (err: unknown) {
      if (!ticket.current) return;
      error = err instanceof Error ? err.message : String(err);
      taskList = [];
    } finally {
      if (ticket.current) loading = false;
    }
  }

  $effect(() => {
    if (agentId) {
      void fetchTasks();
    }
  });
</script>

<section data-testid="agent-detail-activity">
  <h3 class="mb-3 text-sm font-medium">{$t('agent_detail.activity_title')}</h3>

  {#if loading}
    <p class="text-xs text-muted-foreground">{$t('common.loading')}</p>
  {:else if error}
    <p class="text-xs text-destructive">{error}</p>
  {:else if taskList.length === 0}
    <p class="text-xs text-muted-foreground">{$t('agent_detail.no_activity')}</p>
  {:else}
    <div class="space-y-1.5">
      {#each taskList as task (task.id)}
        <button
          class="flex w-full items-center gap-3 rounded-md border px-3 py-2 text-left text-sm transition-colors hover:bg-muted/50"
          onclick={() => onTaskClick(task.id)}
          data-testid="agent-detail-task-row"
          data-task-id={task.id}
        >
          <Badge variant={TASK_STATUS_VARIANT[task.status] ?? "secondary"} class="shrink-0 text-[10px]">
            {task.status}
          </Badge>
          <SmartOutputPreview
            output={task.output_text ?? task.input_preview ?? "-"}
            maxLength={80}
            class="min-w-0 flex-1 text-xs"
          />
          <span class="shrink-0 text-xs text-muted-foreground">{formatDuration(task.duration_ms)}</span>
          <span class="shrink-0 text-xs text-muted-foreground">{formatRelativeTime(task.created_at)}</span>
        </button>
      {/each}
    </div>
  {/if}
</section>
