<script lang="ts">
  import { onMount } from "svelte";
  import { t } from "svelte-i18n";
  import { Select } from "$lib/components/ui/select";
  import { ErrorBanner } from "$lib/components/operator";
  import { reportError } from "$lib/errors/reportError";
  import type { HumanizedError } from "$lib/errors/humanize";
  import {
    listTasks,
    getDelegationTree,
    type DelegationNode,
    type DelegationTaskOption,
  } from "$lib/ipc/delegation";

  let tasks = $state<DelegationTaskOption[]>([]);
  let selectedTaskId = $state<string>("");
  let tree = $state<DelegationNode | null>(null);
  let loadingTasks = $state(true);
  let loadingTree = $state(false);
  let errState = $state<HumanizedError | null>(null);

  const STATUS_COLORS: Record<string, string> = {
    running: "bg-info/15 text-info border-info/30",
    working: "bg-info/15 text-info border-info/30",
    submitted: "bg-muted text-muted-foreground border-border",
    completed: "bg-success/15 text-success border-success/30",
    failed: "bg-destructive/15 text-destructive border-destructive/30",
    canceled: "bg-muted text-muted-foreground border-border",
  };

  function statusClass(status: string): string {
    return STATUS_COLORS[status] ?? "bg-muted text-muted-foreground border-border";
  }

  let hasDelegation = $derived(tree !== null && tree.children.length > 0);

  async function loadTasks() {
    loadingTasks = true;
    try {
      const summaries = await listTasks();
      tasks = summaries;
      if (!selectedTaskId && summaries.length > 0) {
        selectedTaskId = summaries[0].id;
        void loadTree(selectedTaskId);
      }
    } catch (e) {
      errState = reportError(e, { surface: "inline" });
    } finally {
      loadingTasks = false;
    }
  }

  async function loadTree(taskId: string) {
    if (!taskId) return;
    loadingTree = true;
    errState = null;
    try {
      tree = await getDelegationTree(taskId);
    } catch (e) {
      errState = reportError(e, { surface: "inline" });
      tree = null;
    } finally {
      loadingTree = false;
    }
  }

  function onSelectChange(event: Event) {
    const target = event.target as HTMLSelectElement;
    selectedTaskId = target.value;
    void loadTree(selectedTaskId);
  }

  onMount(() => {
    void loadTasks();
  });
</script>

<div class="space-y-4" data-testid="delegation-tree">
  <div class="flex items-center justify-between gap-3">
    <h2 class="text-lg font-semibold">{$t("observability.delegation_title")}</h2>
    {#if !loadingTasks && tasks.length > 0}
      <Select
        class="rounded-md border border-border bg-background px-3 py-1.5 text-sm"
        value={selectedTaskId}
        onchange={onSelectChange}
        data-testid="delegation-task-select"
      >
        {#each tasks as task (task.id)}
          <option value={task.id}>
            {task.agent_name || task.id.slice(0, 8)} - {task.status} ({task.id.slice(0, 8)})
          </option>
        {/each}
      </Select>
    {/if}
  </div>

  {#if loadingTasks}
    <p class="text-sm text-muted-foreground">{$t("observability.delegation_loading")}</p>
  {:else if tasks.length === 0}
    <p class="text-sm text-muted-foreground">{$t("observability.delegation_no_tasks")}</p>
  {:else if loadingTree}
    <p class="text-sm text-muted-foreground">{$t("observability.delegation_loading")}</p>
  {:else if errState}
    <ErrorBanner
      message={errState.friendly_message}
      onretry={() => void loadTasks()}
      retryLabel={$t('common.retry')}
      data-testid="delegation-error"
    />
  {:else if tree}
    <div class="rounded-lg border border-border bg-card p-4">
      <div class="flex items-center gap-2">
        <span class="text-base" aria-hidden="true">●</span>
        <span class="font-mono text-sm font-medium" data-testid="delegation-root">
          {tree.agent_name}
        </span>
        <span class="text-xs text-muted-foreground">
          ({$t("observability.delegation_root_label")})
        </span>
        <span class="rounded-md border px-2 py-0.5 text-xs {statusClass(tree.status)}">
          {tree.status}
        </span>
        {#if tree.started_at}
          <span class="text-xs text-muted-foreground">{tree.started_at}</span>
        {/if}
      </div>

      {#if hasDelegation}
        <ul class="mt-3 space-y-1.5 border-l border-border pl-4">
          {#each tree.children as child, i (i)}
            <li class="flex items-center gap-2" data-testid="delegation-child">
              <span class="text-muted-foreground" aria-hidden="true">↳</span>
              <span class="font-mono text-sm">{child.agent_name}</span>
              <span class="rounded-md border px-2 py-0.5 text-xs {statusClass(child.status)}">
                {child.status}
              </span>
              {#if child.started_at}
                <span class="text-xs text-muted-foreground">{child.started_at}</span>
              {/if}
            </li>
          {/each}
        </ul>
      {:else}
        <p class="mt-3 text-sm text-muted-foreground" data-testid="delegation-empty">
          {$t("observability.delegation_empty")}
        </p>
      {/if}
    </div>
  {:else}
    <p class="text-sm text-muted-foreground">{$t("observability.delegation_select_task")}</p>
  {/if}
</div>
