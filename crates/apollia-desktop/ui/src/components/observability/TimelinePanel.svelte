<script lang="ts">
  /**
   * `TimelinePanel` - the body of the Observability "Chronologie" tab.
   *
   * Holds the reading scope: the whole activity window (`TimelineGlobal`) or a
   * single task (`TaskTimeline`). The scope is controlled by the page, so an
   * external jump ("View logs" on a failed run) can land here already focused
   * on the task that failed.
   */
  import { onMount } from "svelte";
  import { t } from "svelte-i18n";
  import { Select } from "$lib/components/ui/select";
  import { Card } from "$lib/components/ui/card";
  import { listTasks } from "$lib/ipc/tasks";
  import type { TaskSummary } from "$lib/types";
  import { reportError } from "$lib/errors/reportError";
  import TimelineGlobal from "./TimelineGlobal.svelte";
  import TaskTimeline from "./TaskTimeline.svelte";

  interface Props {
    /** Focused task, `null` for the whole activity window. */
    taskId: string | null;
    /** Raised when the operator changes the reading scope. */
    ontaskchange: (taskId: string | null) => void;
  }

  let { taskId, ontaskchange }: Props = $props();

  let tasks = $state<TaskSummary[]>([]);
  let tasksLoaded = $state(false);

  let scope = $state<"all" | "task">(taskId === null ? "all" : "task");

  // A focused task arriving from another surface switches the scope on its own,
  // so the operator lands on the timeline of the run they clicked.
  $effect(() => {
    if (taskId !== null) scope = "task";
  });

  /** Label of one option: agent, status, and the short id as the tiebreaker. */
  function optionLabel(task: TaskSummary): string {
    const agent = task.agent_name || task.agent_id;
    return `${agent} · ${task.status} · ${task.id.slice(0, 8)}`;
  }

  async function loadTasks(): Promise<void> {
    try {
      tasks = await listTasks();
    } catch (error: unknown) {
      // A missing task list only costs the picker: a focused task id received
      // from another surface still renders its timeline.
      reportError(error, { surface: "toast", testid: "timeline-tasks-error" });
    } finally {
      tasksLoaded = true;
    }
  }

  function selectAll(): void {
    scope = "all";
    ontaskchange(null);
  }

  // Narrowing the scope is the moment the picker starts being read, and runs
  // keep appearing while this page stays open. Re-reading the list here means
  // the operator chooses among the runs that exist now, and that the task
  // opened by default comes from that fresh list rather than from the window
  // captured when the tab was mounted.
  async function selectTaskScope(): Promise<void> {
    scope = "task";
    await loadTasks();
    if (taskId !== null) return;
    const first = tasks[0];
    if (first !== undefined) ontaskchange(first.id);
  }

  function onSelectChange(event: Event): void {
    const target = event.target as HTMLSelectElement;
    ontaskchange(target.value === "" ? null : target.value);
  }

  onMount(() => {
    void loadTasks();
  });
</script>

<div class="space-y-5" data-testid="timeline-panel">
  <Card class="flex flex-wrap items-center gap-x-5 gap-y-3 px-4 py-3">
    <div class="flex items-center gap-2">
      <span class="section-meta">{$t("observability.task_timeline_scope_label")}</span>
      <div
        class="inline-flex items-center gap-0.5 rounded-md glass-border glass-surface p-0.5"
        role="tablist"
        aria-label={$t("observability.task_timeline_scope_label")}
      >
        <button
          type="button"
          role="tab"
          aria-selected={scope === "all"}
          class="rounded px-2.5 py-1 text-caption font-medium transition-colors {scope === 'all'
            ? 'bg-background text-foreground shadow-sm'
            : 'text-muted-foreground hover:text-foreground'}"
          onclick={selectAll}
          data-testid="timeline-scope-all"
        >
          {$t("observability.task_timeline_scope_all")}
        </button>
        <button
          type="button"
          role="tab"
          aria-selected={scope === "task"}
          class="rounded px-2.5 py-1 text-caption font-medium transition-colors {scope === 'task'
            ? 'bg-background text-foreground shadow-sm'
            : 'text-muted-foreground hover:text-foreground'}"
          onclick={() => void selectTaskScope()}
          data-testid="timeline-scope-task"
        >
          {$t("observability.task_timeline_scope_task")}
        </button>
      </div>
    </div>

    {#if scope === "task"}
      <div class="ml-auto flex items-center gap-2">
        <label for="timeline-task-select" class="section-meta">
          {$t("observability.task_timeline_select_label")}
        </label>
        <Select
          id="timeline-task-select"
          class="h-8 w-auto"
          value={taskId ?? ""}
          onchange={onSelectChange}
          data-testid="timeline-task-select"
        >
          {#if taskId !== null && !tasks.some((task) => task.id === taskId)}
            <!-- The focused task can predate the loaded page, or be gone from
                 it: keep it selectable so the timeline below stays readable. -->
            <option value={taskId}>{taskId.slice(0, 8)}</option>
          {/if}
          {#each tasks as task (task.id)}
            <option value={task.id}>{optionLabel(task)}</option>
          {/each}
        </Select>
      </div>
    {/if}
  </Card>

  {#if scope === "all"}
    <TimelineGlobal />
  {:else if taskId !== null}
    <TaskTimeline {taskId} />
  {:else if tasksLoaded && tasks.length === 0}
    <Card class="flex flex-col items-center justify-center py-12" data-testid="timeline-no-tasks">
      <p class="text-body-sm text-muted-foreground">
        {$t("observability.task_timeline_no_tasks")}
      </p>
    </Card>
  {:else}
    <Card class="flex flex-col items-center justify-center py-12" data-testid="timeline-pick-task">
      <p class="text-body-sm text-muted-foreground">
        {$t("observability.task_timeline_pick")}
      </p>
    </Card>
  {/if}
</div>
