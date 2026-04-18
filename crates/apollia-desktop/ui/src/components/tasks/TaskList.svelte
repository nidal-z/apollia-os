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
  import { navigateTo } from "$lib/stores/navigation";
  import { formatRelativeTime, formatDuration } from "$lib/utils";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import { Skeleton } from "$lib/components/ui/skeleton";
  import { Select } from "$lib/components/ui/select";
  import { Textarea } from "$lib/components/ui/textarea";
  import { Dialog } from "$lib/components/ui/dialog";
  import { Plus, Activity, CheckCircle, XCircle, Clock, AlertTriangle, Ban } from "lucide-svelte";
  import { EmptyState } from "$lib/components/layout";
  import { EMPTY_STATES } from "$lib/i18n/strings/empty-states";

  const SKELETON_ROW_COUNT = 6;

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

  const STATUS_ICON: Record<string, { icon: typeof Activity; color: string }> = {
    working: { icon: Activity, color: "text-primary" },
    submitted: { icon: Clock, color: "text-muted-foreground" },
    completed: { icon: CheckCircle, color: "text-success" },
    failed: { icon: XCircle, color: "text-destructive" },
    input_required: { icon: AlertTriangle, color: "text-warning" },
    canceled: { icon: Ban, color: "text-muted-foreground" },
  };

  const STATUS_BADGE: Record<string, { variant: "success" | "destructive" | "warning" | "info" | "secondary" }> = {
    completed: { variant: "success" },
    working: { variant: "info" },
    submitted: { variant: "secondary" },
    failed: { variant: "destructive" },
    input_required: { variant: "warning" },
    canceled: { variant: "secondary" },
  };

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
    return mode === "operator" ? ALL_TABS.filter((tab) => !tab.operatorHidden) : ALL_TABS;
  });

  const STATUS_I18N: Record<string, string> = {
    working: "dashboard.status_working",
    submitted: "dashboard.status_submitted",
    completed: "dashboard.status_completed",
    failed: "dashboard.status_failed",
    input_required: "dashboard.status_approval",
    canceled: "dashboard.status_canceled",
  };

  function statusLabel(status: string): string {
    if (mode === "operator") {
      if (status === "working") return $t("tasks.tab_in_progress");
      if (status === "input_required") return $t("tasks.tab_needs_approval");
    }
    return $t(STATUS_I18N[status] ?? "dashboard.status_submitted");
  }

  let filteredTasks = $derived.by<TaskSummary[]>(() => {
    let result = $tasks;
    if (activeTab !== "all") result = result.filter((task) => task.status === activeTab);
    if (filterAgentId) result = result.filter((task) => task.agent_id === filterAgentId);
    return result;
  });

  let visibleTasks = $derived<TaskSummary[]>(filteredTasks.slice(0, visibleCount));
  let hasMore = $derived(filteredTasks.length > visibleCount);
  let hasAnyTasks = $derived($tasks.length > 0);
  let activeAgents = $derived($agents.filter((a) => a.runtime_status === "active" || a.runtime_status === "degraded"));

  let uniqueAgents = $derived.by<{ id: string; name: string }[]>(() => {
    const seen = new Map<string, string>();
    for (const task of $tasks) {
      if (!seen.has(task.agent_id)) seen.set(task.agent_id, task.agent_name || task.agent_id);
    }
    return Array.from(seen.entries()).map(([id, name]) => ({ id, name }));
  });

  function shortId(id: string): string { return id.slice(0, SHORT_ID_LENGTH); }
  function handleTabChange(tab: StatusTab) { activeTab = tab; visibleCount = PAGE_SIZE; }
  function loadMore() { visibleCount += PAGE_SIZE; }

  function openNewTaskDialog() {
    showNewTaskDialog = true; newTaskAgentId = ""; newTaskInput = ""; submitError = null;
  }

  async function handleSubmitTask() {
    if (!newTaskAgentId || !newTaskInput.trim()) return;
    submitting = true; submitError = null;
    try {
      const taskId: string = await invoke("submit_task", { agentId: newTaskAgentId, input: newTaskInput.trim() });
      showNewTaskDialog = false;
      onSelectTask(taskId);
    } catch (err: unknown) {
      submitError = err instanceof Error ? err.message : String(err);
    } finally {
      submitting = false;
    }
  }

  let inputCharCount = $derived(newTaskInput.length);
</script>

<!-- Toolbar: tabs + filter + new task -->
<div class="flex flex-wrap items-center gap-3">
  <!-- Segmented tabs -->
  <div class="inline-flex rounded-lg bg-muted p-0.5">
    {#each visibleTabs as tab (tab.key)}
      <button
        class="rounded-md px-3 py-1 text-xs font-medium transition-all duration-150 {activeTab === tab.key
          ? 'bg-card text-foreground shadow-sm'
          : 'text-muted-foreground hover:text-foreground'}"
        onclick={() => handleTabChange(tab.key)}
      >
        {$t(mode === "operator" && tab.operatorLabelKey ? tab.operatorLabelKey : tab.labelKey)}
      </button>
    {/each}
  </div>

  <!-- Agent filter -->
  <Select class="h-8 w-auto text-xs" bind:value={filterAgentId}>
    <option value="">{$t('tasks.all_agents')}</option>
    {#each uniqueAgents as agent}
      <option value={agent.id}>{agent.name}</option>
    {/each}
  </Select>

  <!-- Counter -->
  <span class="text-[11px] text-muted-foreground/50">{filteredTasks.length} {$t('tasks.count_suffix')}</span>

  <!-- New task button -->
  <div class="ml-auto">
    <Button size="sm" onclick={openNewTaskDialog} data-testid="new-task-btn" class="gap-1 text-xs h-8">
      <Plus size={12} />
      {$t('tasks.new_task')}
    </Button>
  </div>
</div>

<!-- Task list -->
<div class="mt-4">
  {#if $connectionStatus === "connecting"}
    <div class="glass-card glass-border rounded-lg overflow-hidden" data-testid="task-list-skeleton">
      {#each { length: SKELETON_ROW_COUNT } as _, i}
        <div class="flex items-center gap-3 px-4 py-3 {i > 0 ? 'border-t border-border/40' : ''}">
          <Skeleton class="h-3 w-3 rounded-full" />
          <Skeleton class="h-3 w-24" />
          <Skeleton class="h-4 w-16 rounded-full" />
          <Skeleton class="h-3 flex-1" />
          <Skeleton class="h-3 w-12" />
          <Skeleton class="h-3 w-14" />
        </div>
      {/each}
    </div>
  {:else if !hasAnyTasks}
    <EmptyState
      icon={EMPTY_STATES.tasks.icon}
      title={$t(EMPTY_STATES.tasks.titleKey)}
      description={$t(EMPTY_STATES.tasks.descriptionKey)}
      primaryLabel={mode === "operator" ? $t('tasks.empty_cta_operator') : $t(EMPTY_STATES.tasks.primaryCtaKey ?? '')}
      primaryAction={mode === "operator" ? () => navigateTo("agents") : openNewTaskDialog}
      secondaryLabel={mode === "operator" ? undefined : $t(EMPTY_STATES.tasks.secondaryCtaKey ?? '')}
      secondaryAction={mode === "operator" ? undefined : () => navigateTo("agents")}
      page="tasks"
    />
  {:else if visibleTasks.length === 0}
    <div class="glass-card glass-border rounded-lg px-4 py-10 text-center">
      <p class="text-xs text-muted-foreground">{$t('tasks.no_match')}</p>
    </div>
  {:else}
    <!-- Column headers -->
    <div class="flex items-center gap-3 px-4 py-1.5 text-[10px] font-medium uppercase tracking-wider text-muted-foreground/50" data-testid="task-list-header">
      <span class="w-4 shrink-0"></span>
      <span class="w-32 shrink-0">{$t('tasks.header_agent')}</span>
      <span class="w-20 shrink-0">{$t('tasks.header_status')}</span>
      <span class="min-w-0 flex-1">{$t('tasks.header_summary')}</span>
      <span class="w-14 shrink-0 text-right">{$t('tasks.header_duration')}</span>
      <span class="w-16 shrink-0 text-right">{$t('tasks.header_time')}</span>
    </div>

    <!-- Rows -->
    <div class="glass-card glass-border rounded-lg overflow-hidden divide-y divide-border/40">
      {#each visibleTasks as task, i (task.id)}
        {@const iconCfg = STATUS_ICON[task.status] ?? STATUS_ICON.submitted}
        {@const badgeCfg = STATUS_BADGE[task.status] ?? STATUS_BADGE.submitted}
        {@const IconComponent = iconCfg.icon}
        <button
          animate:flip={{ duration: 250 }}
          in:fly={{ y: 4, duration: 150, delay: Math.min(i, 10) * 20 }}
          class="list-item-spring group flex w-full items-center gap-3 px-4 py-2.5 text-left text-sm hover:bg-primary/5"
          data-testid="task-row"
          data-task-id={task.id}
          data-task-status={task.status}
          onclick={() => onSelectTask(task.id)}
        >
          <!-- Status icon -->
          <IconComponent size={13} class="{iconCfg.color} shrink-0 {task.status === 'working' ? 'animate-spin' : ''}" />

          <!-- Agent name + short ID -->
          <span class="w-32 shrink-0 truncate">
            <span class="text-xs font-medium">{task.agent_name || task.agent_id}</span>
            <code class="ml-1 text-[9px] text-muted-foreground/40">{shortId(task.id)}</code>
          </span>

          <!-- Status badge -->
          <span class="w-20 shrink-0">
            <Badge variant={badgeCfg.variant} class="text-[9px] px-1.5 py-0">
              {statusLabel(task.status)}
            </Badge>
          </span>

          <!-- Summary -->
          <span class="min-w-0 flex-1 truncate text-xs text-muted-foreground/60">
            {#if task.output_text}
              {task.output_text.slice(0, MAX_SUMMARY_LENGTH)}
            {:else if task.input_preview}
              <span class="italic">{task.input_preview.slice(0, MAX_SUMMARY_LENGTH)}</span>
            {:else}
              -
            {/if}
          </span>

          <!-- Duration -->
          <span class="w-14 shrink-0 text-right text-[11px] text-muted-foreground/50">{formatDuration(task.duration_ms)}</span>

          <!-- Time -->
          <span class="w-16 shrink-0 text-right text-[11px] text-muted-foreground/50">{formatRelativeTime(task.created_at)}</span>
        </button>
      {/each}
    </div>

    {#if hasMore}
      <div class="flex justify-center pt-3">
        <Button size="sm" variant="ghost" onclick={loadMore} class="text-xs">{$t('common.load_more')}</Button>
      </div>
    {/if}
  {/if}
</div>

<!-- New Task Dialog -->
<Dialog open={showNewTaskDialog} onclose={() => { showNewTaskDialog = false; }} size="sm" title={$t('tasks.new_task')} data-testid="new-task-dialog">
  <div class="space-y-4">
    <div>
      <label class="mb-1.5 block text-xs font-medium text-muted-foreground" for="new-task-agent">{$t('tasks.agent_label')}</label>
      <Select id="new-task-agent" data-testid="new-task-agent-select" bind:value={newTaskAgentId}>
        <option value="" disabled>{$t('tasks.select_agent')}</option>
        {#each activeAgents as agent}
          <option value={agent.id}>{agent.name}</option>
        {/each}
      </Select>
    </div>

    <div>
      <label class="mb-1.5 block text-xs font-medium text-muted-foreground" for="new-task-input">{$t('tasks.input_label')}</label>
      <Textarea
        id="new-task-input"
        rows={5}
        maxlength={MAX_INPUT_LENGTH}
        placeholder={$t('tasks.input_placeholder')}
        data-testid="new-task-input"
        bind:value={newTaskInput}
      />
      <p class="mt-1 text-[10px] text-muted-foreground/50">{inputCharCount} / {MAX_INPUT_LENGTH}</p>
    </div>

    {#if submitError}
      <p class="text-xs text-destructive">{submitError}</p>
    {/if}

    <div class="flex justify-end gap-2">
      <Button variant="outline" size="sm" onclick={() => { showNewTaskDialog = false; }}>{$t('common.cancel')}</Button>
      <Button size="sm" onclick={handleSubmitTask} disabled={!newTaskAgentId || !newTaskInput.trim() || submitting} data-testid="new-task-submit-btn">
        {submitting ? $t('common.submitting') : $t('common.submit')}
      </Button>
    </div>
  </div>
</Dialog>
