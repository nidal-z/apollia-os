<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import type { TaskSummary } from "$lib/types";
  import { tasks } from "$lib/stores/tasks";
  import { agents } from "$lib/stores/agents";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";

  interface Props {
    onSelectTask: (taskId: string) => void;
  }

  let { onSelectTask }: Props = $props();

  type StatusTab = "all" | "working" | "completed" | "failed" | "input_required";

  const TABS: { key: StatusTab; label: string }[] = [
    { key: "all", label: "All" },
    { key: "working", label: "Running" },
    { key: "completed", label: "Completed" },
    { key: "failed", label: "Failed" },
    { key: "input_required", label: "Pending Approval" },
  ];

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
    input_required: "border-[var(--apollia-warning)] text-[var(--apollia-warning)]",
  };

  const PAGE_SIZE = 50;

  let activeTab = $state<StatusTab>("all");
  let filterAgentId = $state<string>("");
  let visibleCount = $state(PAGE_SIZE);

  let showNewTaskDialog = $state(false);
  let newTaskAgentId = $state("");
  let newTaskInput = $state("");
  let submitting = $state(false);
  let submitError = $state<string | null>(null);

  let filteredTasks = $derived.by<TaskSummary[]>(() => {
    let result = $tasks;
    if (activeTab !== "all") {
      result = result.filter((t) => t.status === activeTab);
    }
    if (filterAgentId) {
      result = result.filter((t) => t.agent_id === filterAgentId);
    }
    return result;
  });

  let visibleTasks = $derived<TaskSummary[]>(filteredTasks.slice(0, visibleCount));
  let hasMore = $derived(filteredTasks.length > visibleCount);

  let activeAgents = $derived($agents.filter((a) => a.state === "active" || a.state === "degraded"));

  let uniqueAgents = $derived.by<{ id: string; name: string }[]>(() => {
    const seen = new Map<string, string>();
    for (const t of $tasks) {
      if (!seen.has(t.agent_id)) {
        seen.set(t.agent_id, t.agent_name || t.agent_id);
      }
    }
    return Array.from(seen.entries()).map(([id, name]) => ({ id, name }));
  });

  function shortId(id: string): string {
    return id.slice(0, 8);
  }

  function formatDuration(ms: number | undefined): string {
    if (ms === undefined || ms === null) return "-";
    if (ms < 1000) return `${ms}ms`;
    if (ms < 60_000) return `${(ms / 1000).toFixed(1)}s`;
    const mins = Math.floor(ms / 60_000);
    const secs = Math.floor((ms % 60_000) / 1000);
    return `${mins}m ${secs}s`;
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
    return new Date(isoDate).toLocaleDateString();
  }

  function handleTabChange(tab: StatusTab) {
    activeTab = tab;
    visibleCount = PAGE_SIZE;
  }

  function loadMore() {
    visibleCount += PAGE_SIZE;
  }

  function openNewTaskDialog() {
    showNewTaskDialog = true;
    newTaskAgentId = "";
    newTaskInput = "";
    submitError = null;
  }

  function closeNewTaskDialog() {
    showNewTaskDialog = false;
  }

  async function handleSubmitTask() {
    if (!newTaskAgentId || !newTaskInput.trim()) return;

    submitting = true;
    submitError = null;
    try {
      const taskId: string = await invoke("submit_task", {
        agentId: newTaskAgentId,
        input: newTaskInput.trim(),
      });
      showNewTaskDialog = false;
      onSelectTask(taskId);
    } catch (err: unknown) {
      submitError = err instanceof Error ? err.message : String(err);
    } finally {
      submitting = false;
    }
  }

  const MAX_INPUT_LENGTH = 4000;
  let inputCharCount = $derived(newTaskInput.length);
</script>

<div class="space-y-4">
  <!-- Tabs + agent filter + New Task button -->
  <div class="flex flex-wrap items-center gap-3">
    <div class="flex gap-1 rounded-md border bg-muted/30 p-1">
      {#each TABS as tab}
        <button
          class="rounded px-3 py-1 text-sm font-medium transition-colors {activeTab === tab.key
            ? 'bg-background text-foreground shadow-sm'
            : 'text-muted-foreground hover:text-foreground'}"
          onclick={() => handleTabChange(tab.key)}
        >
          {tab.label}
        </button>
      {/each}
    </div>

    <select
      class="rounded-md border bg-background px-3 py-1.5 text-sm"
      bind:value={filterAgentId}
    >
      <option value="">All agents</option>
      {#each uniqueAgents as agent}
        <option value={agent.id}>{agent.name}</option>
      {/each}
    </select>

    <div class="ml-auto">
      <Button size="sm" onclick={openNewTaskDialog} data-testid="new-task-btn">New Task</Button>
    </div>
  </div>

  <!-- Task list -->
  {#if visibleTasks.length === 0}
    <div class="flex flex-col items-center justify-center gap-2 rounded-lg border border-dashed py-12">
      <p class="text-muted-foreground">No tasks match the current filters.</p>
    </div>
  {:else}
    <div class="space-y-1">
      {#each visibleTasks as task (task.id)}
        <button
          class="flex w-full items-center gap-3 rounded-md border px-3 py-2 text-left text-sm transition-colors hover:bg-accent/50"
          data-testid="task-row"
          data-task-id={task.id}
          data-task-status={task.status}
          onclick={() => onSelectTask(task.id)}
        >
          <code class="shrink-0 text-xs text-muted-foreground">{shortId(task.id)}</code>
          <span class="min-w-0 flex-1 truncate">{task.agent_name || task.agent_id}</span>
          <Badge
            variant={STATUS_VARIANT[task.status] ?? "secondary"}
            class="shrink-0 text-[10px] {STATUS_EXTRA_CLASS[task.status] ?? ''}"
          >
            {task.status}
          </Badge>
          <span class="shrink-0 text-xs text-muted-foreground">{formatDuration(task.duration_ms)}</span>
          <span class="shrink-0 text-xs text-muted-foreground">{formatRelativeTime(task.created_at)}</span>
        </button>
      {/each}
    </div>

    {#if hasMore}
      <div class="flex justify-center pt-2">
        <Button size="sm" variant="outline" onclick={loadMore}>Load more</Button>
      </div>
    {/if}
  {/if}
</div>

<!-- New Task Dialog -->
{#if showNewTaskDialog}
  <div
    class="fixed inset-0 z-50 flex items-center justify-center bg-black/50"
    role="button"
    tabindex="-1"
    onclick={closeNewTaskDialog}
    onkeydown={(e) => e.key === "Escape" && closeNewTaskDialog()}
  >
    <div
      class="w-[480px] rounded-lg border bg-background p-6 shadow-lg"
      role="dialog"
      aria-modal="true"
      tabindex="-1"
      data-testid="new-task-dialog"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.key === "Escape" && closeNewTaskDialog()}
    >
      <h3 class="mb-4 text-lg font-semibold">New Task</h3>

      <div class="space-y-4">
        <div>
          <label class="mb-1 block text-sm font-medium" for="new-task-agent">Agent</label>
          <select
            id="new-task-agent"
            class="w-full rounded-md border bg-background px-3 py-2 text-sm"
            data-testid="new-task-agent-select"
            bind:value={newTaskAgentId}
          >
            <option value="" disabled>Select an agent...</option>
            {#each activeAgents as agent}
              <option value={agent.id}>{agent.name}</option>
            {/each}
          </select>
        </div>

        <div>
          <label class="mb-1 block text-sm font-medium" for="new-task-input">Input</label>
          <textarea
            id="new-task-input"
            class="w-full rounded-md border bg-background px-3 py-2 text-sm"
            rows="6"
            maxlength={MAX_INPUT_LENGTH}
            placeholder="Describe the task for the agent..."
            data-testid="new-task-input"
            bind:value={newTaskInput}
          ></textarea>
          <p class="mt-1 text-xs text-muted-foreground">
            {inputCharCount} / {MAX_INPUT_LENGTH}
          </p>
        </div>

        {#if submitError}
          <p class="text-sm text-[hsl(var(--destructive))]">{submitError}</p>
        {/if}

        <div class="flex justify-end gap-2">
          <Button variant="outline" size="sm" onclick={closeNewTaskDialog}>Cancel</Button>
          <Button
            size="sm"
            onclick={handleSubmitTask}
            disabled={!newTaskAgentId || !newTaskInput.trim() || submitting}
            data-testid="new-task-submit-btn"
          >
            {submitting ? "Submitting..." : "Submit"}
          </Button>
        </div>
      </div>
    </div>
  </div>
{/if}
