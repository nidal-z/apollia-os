<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { fly } from "svelte/transition";
  import { flip } from "svelte/animate";
  import { t } from "svelte-i18n";
  import type { TaskSummary } from "$lib/types";
  import { tasks } from "$lib/stores/tasks";
  import { agents } from "$lib/stores/agents";
  import { connectionStatus } from "$lib/stores/sse";
  import { uiMode } from "$lib/stores/mode";
  import { currentRoute } from "$lib/stores/navigation";
  import { formatRelativeTime, formatDuration } from "$lib/utils";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import { Skeleton } from "$lib/components/ui/skeleton";
  import { ListChecks } from "lucide-svelte";
  import SmartOutputPreview from "../common/SmartOutputPreview.svelte";
  import EmptyState from "../common/EmptyState.svelte";

  const SKELETON_ROW_COUNT = 5;

  interface Props {
    onSelectTask: (taskId: string) => void;
  }

  let { onSelectTask }: Props = $props();

  type StatusTab = "all" | "submitted" | "working" | "completed" | "failed" | "input_required" | "canceled";

  const PAGE_SIZE = 50;
  const MAX_INPUT_LENGTH = 4000;
  const SHORT_ID_LENGTH = 8;
  const MAX_SUMMARY_LENGTH = 80;

  let activeTab = $state<StatusTab>("all");
  let filterAgentId = $state<string>("");
  let visibleCount = $state(PAGE_SIZE);

  let showNewTaskDialog = $state(false);
  let newTaskAgentId = $state("");
  let newTaskInput = $state("");
  let submitting = $state(false);
  let submitError = $state<string | null>(null);

  let mode = $derived($uiMode);

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
    working: "animate-pulse border-info text-info",
    input_required: "border-[var(--apollia-warning)] text-[var(--apollia-warning)]",
  };

  /** Tab definitions — filtered by mode in the derived below. */
  const ALL_TABS: { key: StatusTab; labelKey: string; operatorLabelKey?: string; operatorHidden?: boolean }[] = [
    { key: "all", labelKey: "tasks.tab_all" },
    { key: "submitted", labelKey: "tasks.tab_submitted", operatorHidden: true },
    { key: "working", labelKey: "tasks.tab_running", operatorLabelKey: "tasks.tab_in_progress" },
    { key: "completed", labelKey: "tasks.tab_completed" },
    { key: "failed", labelKey: "tasks.tab_failed" },
    { key: "input_required", labelKey: "tasks.tab_pending_approval", operatorLabelKey: "tasks.tab_needs_approval" },
    { key: "canceled", labelKey: "tasks.tab_canceled", operatorHidden: true },
  ];

  let visibleTabs = $derived.by(() => {
    if (mode === "operator") {
      return ALL_TABS.filter((tab) => !tab.operatorHidden);
    }
    return ALL_TABS;
  });

  /** Status display label — mode-aware (AC-6). */
  function statusLabel(status: string): string {
    if (mode === "operator") {
      if (status === "working") return $t("tasks.tab_in_progress");
      if (status === "input_required") return $t("tasks.tab_needs_approval");
    }
    return status;
  }

  let filteredTasks = $derived.by<TaskSummary[]>(() => {
    let result = $tasks;
    if (activeTab !== "all") {
      result = result.filter((task) => task.status === activeTab);
    }
    if (filterAgentId) {
      result = result.filter((task) => task.agent_id === filterAgentId);
    }
    return result;
  });

  let visibleTasks = $derived<TaskSummary[]>(filteredTasks.slice(0, visibleCount));
  let hasMore = $derived(filteredTasks.length > visibleCount);
  let hasAnyTasks = $derived($tasks.length > 0);

  let activeAgents = $derived($agents.filter((a) => a.runtime_status === "active" || a.runtime_status === "degraded"));

  let uniqueAgents = $derived.by<{ id: string; name: string }[]>(() => {
    const seen = new Map<string, string>();
    for (const task of $tasks) {
      if (!seen.has(task.agent_id)) {
        seen.set(task.agent_id, task.agent_name || task.agent_id);
      }
    }
    return Array.from(seen.entries()).map(([id, name]) => ({ id, name }));
  });

  function shortId(id: string): string {
    return id.slice(0, SHORT_ID_LENGTH);
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

  function navigateToAgents() {
    $currentRoute = "agents";
  }

  let inputCharCount = $derived(newTaskInput.length);
</script>

<div class="space-y-4">
  <!-- Tabs + agent filter + New Task button -->
  <div class="flex flex-wrap items-center gap-3">
    <div class="flex gap-1 rounded-md glass-border glass-surface p-1">
      {#each visibleTabs as tab}
        <button
          class="rounded px-3 py-1 text-sm font-medium transition-colors {activeTab === tab.key
            ? 'glass-inset text-foreground shadow-sm'
            : 'text-muted-foreground hover:text-foreground'}"
          onclick={() => handleTabChange(tab.key)}
        >
          {$t(mode === "operator" && tab.operatorLabelKey ? tab.operatorLabelKey : tab.labelKey)}
        </button>
      {/each}
    </div>

    <select
      class="rounded-md glass-border glass-surface px-3 py-1.5 text-sm"
      bind:value={filterAgentId}
    >
      <option value="">{$t('tasks.all_agents')}</option>
      {#each uniqueAgents as agent}
        <option value={agent.id}>{agent.name}</option>
      {/each}
    </select>

    <div class="ml-auto">
      <Button size="sm" onclick={openNewTaskDialog} data-testid="new-task-btn">{$t('tasks.new_task')}</Button>
    </div>
  </div>

  <!-- Task list, skeleton loaders, or empty state -->
  {#if $connectionStatus === "connecting"}
    <div class="space-y-1" data-testid="task-list-skeleton">
      <div class="flex items-center gap-3 px-3 py-1">
        <Skeleton class="h-3 w-[140px]" />
        <Skeleton class="h-3 w-[100px]" />
        <Skeleton class="h-3 flex-1" />
        <Skeleton class="h-3 w-[70px]" />
        <Skeleton class="h-3 w-[80px]" />
      </div>
      {#each { length: SKELETON_ROW_COUNT } as _}
        <div class="flex items-center gap-3 rounded-md glass-border glass-surface px-3 py-2">
          <Skeleton class="h-4 w-[140px]" />
          <Skeleton class="h-5 w-[70px] rounded-full" />
          <Skeleton class="h-3 flex-1" />
          <Skeleton class="h-3 w-[50px]" />
          <Skeleton class="h-3 w-[60px]" />
        </div>
      {/each}
    </div>
  {:else if !hasAnyTasks}
    <EmptyState
      icon={ListChecks}
      title={$t('tasks.empty_title')}
      subtitle={$t('tasks.empty_subtitle')}
      ctaLabel={mode === "operator" ? $t('tasks.empty_cta_operator') : $t('tasks.empty_cta_builder')}
      ctaAction={mode === "operator" ? navigateToAgents : openNewTaskDialog}
      page="tasks"
    />
  {:else if visibleTasks.length === 0}
    <div class="flex flex-col items-center justify-center gap-2 rounded-xl glass-surface glass-border border-dashed py-12">
      <p class="text-muted-foreground">{$t('tasks.no_match')}</p>
    </div>
  {:else}
    <!-- AC-1: Column headers -->
    <div class="space-y-1">
      <div class="flex items-center gap-3 px-3 py-1 text-xs font-medium uppercase tracking-wide text-muted-foreground" data-testid="task-list-header">
        <span class="w-[140px] shrink-0">{$t('tasks.header_agent')}</span>
        <span class="w-[100px] shrink-0">{$t('tasks.header_status')}</span>
        <span class="min-w-0 flex-1">{$t('tasks.header_summary')}</span>
        <span class="w-[70px] shrink-0 text-right">{$t('tasks.header_duration')}</span>
        <span class="w-[80px] shrink-0 text-right">{$t('tasks.header_time')}</span>
      </div>

      {#each visibleTasks as task (task.id)}
        <button
          animate:flip={{ duration: 300 }}
          in:fly={{ y: 10, duration: 200 }}
          class="flex w-full items-center gap-3 rounded-md border px-3 py-2 text-left text-sm transition-colors hover:bg-muted/50"
          data-testid="task-row"
          data-task-id={task.id}
          data-task-status={task.status}
          onclick={() => onSelectTask(task.id)}
        >
          <!-- AC-2: Agent name primary, task short ID secondary -->
          <span class="w-[140px] shrink-0 truncate" title={task.agent_name || task.agent_id}>
            <span class="font-medium">{task.agent_name || task.agent_id}</span>
            <code class="ml-1 text-[10px] text-muted-foreground">{shortId(task.id)}</code>
          </span>

          <!-- Status badge with mode-aware label (AC-6) -->
          <span class="w-[100px] shrink-0">
            <Badge
              variant={STATUS_VARIANT[task.status] ?? "secondary"}
              class="text-[10px] {STATUS_EXTRA_CLASS[task.status] ?? ''}"
            >
              {statusLabel(task.status)}
            </Badge>
          </span>

          <!-- AC-4: Summary via SmartOutputPreview -->
          <span class="min-w-0 flex-1 truncate">
            {#if task.output_text}
              <SmartOutputPreview output={task.output_text} maxLength={MAX_SUMMARY_LENGTH} class="text-xs" />
            {:else if task.input_preview}
              <span class="text-xs text-muted-foreground/70 italic">{task.input_preview}</span>
            {:else}
              <span class="text-xs text-muted-foreground/50">-</span>
            {/if}
          </span>

          <!-- Duration -->
          <span class="w-[70px] shrink-0 text-right text-xs text-muted-foreground">{formatDuration(task.duration_ms)}</span>

          <!-- AC-3: Correct relative timestamp -->
          <span class="w-[80px] shrink-0 text-right text-xs text-muted-foreground">{formatRelativeTime(task.created_at)}</span>
        </button>
      {/each}
    </div>

    {#if hasMore}
      <div class="flex justify-center pt-2">
        <Button size="sm" variant="outline" onclick={loadMore}>{$t('common.load_more')}</Button>
      </div>
    {/if}
  {/if}
</div>

<!-- New Task Dialog -->
{#if showNewTaskDialog}
  <div
    class="fixed inset-0 z-50 flex items-center justify-center bg-black/40 backdrop-blur-sm"
    role="button"
    tabindex="-1"
    onclick={closeNewTaskDialog}
    onkeydown={(e) => e.key === "Escape" && closeNewTaskDialog()}
  >
    <div
      class="w-[480px] rounded-xl glass-card glass-border p-6 shadow-lg"
      role="dialog"
      aria-modal="true"
      tabindex="-1"
      data-testid="new-task-dialog"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.key === "Escape" && closeNewTaskDialog()}
    >
      <h3 class="mb-4 text-lg font-medium">{$t('tasks.new_task')}</h3>

      <div class="space-y-4">
        <div>
          <label class="mb-1 block text-sm font-medium" for="new-task-agent">{$t('tasks.agent_label')}</label>
          <select
            id="new-task-agent"
            class="w-full rounded-md glass-border glass-surface px-3 py-2 text-sm"
            data-testid="new-task-agent-select"
            bind:value={newTaskAgentId}
          >
            <option value="" disabled>{$t('tasks.select_agent')}</option>
            {#each activeAgents as agent}
              <option value={agent.id}>{agent.name}</option>
            {/each}
          </select>
        </div>

        <div>
          <label class="mb-1 block text-sm font-medium" for="new-task-input">{$t('tasks.input_label')}</label>
          <textarea
            id="new-task-input"
            class="w-full rounded-md glass-border glass-surface px-3 py-2 text-sm"
            rows="6"
            maxlength={MAX_INPUT_LENGTH}
            placeholder={$t('tasks.input_placeholder')}
            data-testid="new-task-input"
            bind:value={newTaskInput}
          ></textarea>
          <p class="mt-1 text-xs text-muted-foreground">
            {inputCharCount} / {MAX_INPUT_LENGTH}
          </p>
        </div>

        {#if submitError}
          <p class="text-sm text-destructive">{submitError}</p>
        {/if}

        <div class="flex justify-end gap-2">
          <Button variant="outline" size="sm" onclick={closeNewTaskDialog}>{$t('common.cancel')}</Button>
          <Button
            size="sm"
            onclick={handleSubmitTask}
            disabled={!newTaskAgentId || !newTaskInput.trim() || submitting}
            data-testid="new-task-submit-btn"
          >
            {submitting ? $t('common.submitting') : $t('common.submit')}
          </Button>
        </div>
      </div>
    </div>
  </div>
{/if}
