<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import type { TaskSummary } from "$lib/types";
  import { Sheet } from "$lib/components/ui/sheet";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import { Separator } from "$lib/components/ui/separator";

  interface Props {
    agentId: string;
    open: boolean;
    onclose: () => void;
  }

  let { agentId, open, onclose }: Props = $props();

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
    if (diffSecs < 60) return `${diffSecs}s ago`;
    const diffMins = Math.floor(diffSecs / 60);
    if (diffMins < 60) return `${diffMins}m ago`;
    const diffHours = Math.floor(diffMins / 60);
    return `${diffHours}h ago`;
  }

  async function fetchTasks() {
    loading = true;
    error = null;
    try {
      const result: TaskSummary[] = await invoke("list_tasks", {
        filter: { agent_id: agentId },
      });
      taskList = result.slice(0, 20);
    } catch (err: unknown) {
      const message = err instanceof Error ? err.message : String(err);
      error = message;
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

<Sheet {open} {onclose}>
  <div class="flex h-full flex-col">
    <!-- Header -->
    <div class="flex items-center justify-between px-4 py-3">
      <h2 class="text-lg font-semibold">Agent Logs</h2>
      <Button size="sm" variant="ghost" onclick={onclose}>Close</Button>
    </div>

    <Separator />

    <!-- Content -->
    <div class="flex-1 overflow-auto p-4">
      {#if loading}
        <p class="text-sm text-muted-foreground">Loading tasks...</p>
      {:else if error}
        <p class="text-sm text-[hsl(var(--destructive))]">{error}</p>
      {:else if taskList.length === 0}
        <p class="text-sm text-muted-foreground">No tasks found for this agent.</p>
      {:else}
        <div class="space-y-2">
          {#each taskList as task (task.id)}
            <div class="flex items-center gap-3 rounded-md border px-3 py-2 text-sm">
              <code class="text-xs text-muted-foreground">{shortId(task.id)}</code>
              <Badge variant={TASK_STATUS_VARIANT[task.status] ?? "secondary"} class="text-[10px]">
                {task.status}
              </Badge>
              <span class="text-xs text-muted-foreground">{formatDuration(task.duration_ms)}</span>
              <span class="ml-auto text-xs text-muted-foreground">{formatRelativeTime(task.created_at)}</span>
            </div>
          {/each}
        </div>
      {/if}
    </div>
  </div>
</Sheet>
