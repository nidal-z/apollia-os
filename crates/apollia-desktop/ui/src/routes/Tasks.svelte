<script lang="ts">
  import { onMount } from "svelte";
  import { t, locale } from "svelte-i18n";
  import { fly } from "svelte/transition";
  import type { TaskSummary } from "$lib/types";
  import { tasks, openNewTaskRequested } from "$lib/stores/tasks";
  import { uiMode } from "$lib/stores/mode";
  import { formatRelativeTime } from "$lib/utils";
  import { listTasks, cancelTask, retryTask, deleteTask } from "$lib/ipc/tasks";
  import { reportError } from "$lib/errors/reportError";
  import type { HumanizedError } from "$lib/errors/humanize";
  import ConfirmDialog from "$lib/components/ui/dialog/ConfirmDialog.svelte";
  import { contentIn, contentOut } from "$lib/design/routeTransition";
  import type {
    FilterChip,
    Task as DSTask,
    TaskStatus as DSTaskStatus,
  } from "$lib/components/operator";
  import TasksListView from "../components/tasks/TasksListView.svelte";
  import TasksDetailView from "../components/tasks/TasksDetailView.svelte";
  import TasksFab from "../components/tasks/TasksFab.svelte";

  type Mode = "list" | "split";
  type DetailTab = "overview" | "output" | "trace" | "technical";
  type FilterKey = "all" | "todo" | "running" | "approval" | "done" | "error";

  let mode = $state<Mode>("list");
  let selectedTaskId = $state<string | null>(null);
  let activeTab = $state<DetailTab>("overview");
  let activeFilter = $state<FilterKey>("all");
  let loadState = $state<"loading" | "error" | "ready">("ready");
  let loadError = $state<HumanizedError | null>(null);
  let deleteTarget = $state<TaskSummary | null>(null);

  // ─── Status filters ────────────────────────────────────────────────────────
  const FILTERS: {
    key: FilterKey;
    labelKey: string;
    color: string;
    match: (s: TaskSummary["status"]) => boolean;
  }[] = [
    { key: "all", labelKey: "tasks.filter_all", color: "hsl(var(--muted-foreground))", match: () => true },
    { key: "todo", labelKey: "tasks.filter_todo", color: "hsl(var(--info))", match: (s) => s === "submitted" },
    { key: "running", labelKey: "tasks.filter_running", color: "hsl(var(--primary))", match: (s) => s === "working" },
    { key: "approval", labelKey: "tasks.filter_approval", color: "hsl(var(--warning))", match: (s) => s === "input_required" },
    { key: "done", labelKey: "tasks.filter_done", color: "hsl(var(--success))", match: (s) => s === "completed" },
    { key: "error", labelKey: "tasks.filter_error", color: "hsl(var(--destructive))", match: (s) => s === "failed" },
  ];

  const counts = $derived.by(() => {
    const out: Record<FilterKey, number> = { all: 0, todo: 0, running: 0, approval: 0, done: 0, error: 0 };
    for (const task of $tasks) {
      out.all += 1;
      for (const f of FILTERS) {
        if (f.key === "all") continue;
        if (f.match(task.status)) out[f.key] += 1;
      }
    }
    return out;
  });

  const chips = $derived<FilterChip[]>(
    FILTERS.map((f) => ({
      key: f.key,
      label: $t(f.labelKey),
      color: f.color,
      count: counts[f.key],
      glowWhenActive: f.key === "running",
    })),
  );

  // ─── TaskSummary → DS row adapter ─────────────────────────────────────────
  const STATUS_MAP: Record<TaskSummary["status"], DSTaskStatus> = {
    submitted: "queued",
    working: "running",
    completed: "completed",
    failed: "failed",
    input_required: "awaiting_approval",
    canceled: "cancelled",
  };

  const PROGRESS: Record<TaskSummary["status"], number> = {
    completed: 100,
    working: 45,
    input_required: 20,
    failed: 60,
    submitted: 0,
    canceled: 0,
  };

  function toRow(task: TaskSummary): DSTask {
    return {
      id: task.id,
      title: task.output_text || task.input_preview || ($t("tasks.untitled") as string),
      agent: task.agent_name || task.agent_id,
      status: STATUS_MAP[task.status] ?? "queued",
      progress: PROGRESS[task.status] ?? 0,
      started: formatRelativeTime(task.created_at, $locale ?? "en"),
    };
  }

  const rows = $derived(
    $tasks
      .filter((task) => FILTERS.find((f) => f.key === activeFilter)!.match(task.status))
      .map((task) => ({ task, row: toRow(task) })),
  );

  const inFlightCount = $derived(counts.todo + counts.running);
  const headlineTitle = $derived(
    inFlightCount === 0
      ? ($t("tasks.inflight_zero") as string)
      : inFlightCount === 1
        ? ($t("tasks.inflight_one") as string)
        : ($t("tasks.inflight_many", { values: { n: inFlightCount } }) as string),
  );
  const headlineSubtitle = $derived(
    $uiMode === "operator"
      ? ($t("tasks.headline_subtitle_operator") as string)
      : ($t("tasks.subtitle") as string),
  );

  // ─── Detail state ──────────────────────────────────────────────────────────
  const selectedTask = $derived($tasks.find((task) => task.id === selectedTaskId));
  const isRunning = $derived(
    selectedTask?.status === "working" || selectedTask?.status === "submitted",
  );
  const tabItems = $derived.by(() => {
    const base = [
      { key: "overview", label: $t("tasks.tab_overview") as string },
      { key: "output", label: $t("tasks.tab_output") as string },
      { key: "trace", label: $t("tasks.tab_trace") as string },
    ];
    if ($uiMode === "builder") {
      base.push({ key: "technical", label: $t("tasks.tab_technical") as string });
    }
    return base;
  });

  // ─── Actions ───────────────────────────────────────────────────────────────
  function selectTask(id: string) {
    selectedTaskId = id;
    mode = "split";
    activeTab = "overview";
  }

  function backToList() {
    mode = "list";
    selectedTaskId = null;
  }

  function newTask() {
    openNewTaskRequested.set(Date.now());
  }

  async function refresh(showLoading = false) {
    if (showLoading) loadState = "loading";
    try {
      tasks.set(await listTasks());
      loadError = null;
      loadState = "ready";
    } catch (error) {
      loadError = reportError(error, { surface: "inline" });
      loadState = "error";
    }
  }

  async function cancel(id: string) {
    try {
      await cancelTask(id);
      await refresh();
    } catch (error) {
      reportError(error, { surface: "toast", testid: "task-cancel-error" });
    }
  }

  async function retry(task: TaskSummary) {
    try {
      await retryTask(task.agent_id, task.input_preview);
      await refresh();
    } catch (error) {
      reportError(error, { surface: "toast", testid: "task-retry-error" });
    }
  }

  function requestDelete(task: TaskSummary) {
    deleteTarget = task;
  }

  function cancelDelete() {
    deleteTarget = null;
  }

  async function confirmDelete() {
    const target = deleteTarget;
    if (!target) return;
    deleteTarget = null;
    const wasSelected = selectedTaskId === target.id;
    // Optimistic removal: the list {#each} plays rowOut as the task leaves the store.
    tasks.update((list) => list.filter((task) => task.id !== target.id));
    if (wasSelected) backToList();
    try {
      await deleteTask(target.id);
    } catch (error) {
      reportError(error, { surface: "toast", testid: "task-delete-error" });
      await refresh();
    }
  }

  function isEditableTarget(target: EventTarget | null): boolean {
    const el = target as HTMLElement | null;
    return Boolean(el?.closest?.("input, textarea, select, [contenteditable='true']"));
  }

  function onKeydown(event: KeyboardEvent) {
    if (event.key === "Escape" && mode === "split") {
      backToList();
      return;
    }
    if (
      (event.key === "Delete" || event.key === "Backspace") &&
      mode === "split" &&
      deleteTarget === null &&
      selectedTask !== undefined &&
      !isRunning &&
      selectedTask.status !== "input_required" &&
      !isEditableTarget(event.target)
    ) {
      event.preventDefault();
      requestDelete(selectedTask);
    }
  }

  onMount(() => {
    if ($tasks.length === 0) refresh(true);
  });
</script>

<svelte:window onkeydown={onKeydown} />

<div class="flex h-full min-h-0 w-full flex-col" data-testid="tasks-page">
  {#key mode}
    <div class="flex h-full min-h-0 w-full flex-col" in:fly={contentIn()} out:fly={contentOut()}>
      {#if mode === "list"}
        <TasksListView
          {rows}
          {chips}
          {activeFilter}
          {headlineTitle}
          {headlineSubtitle}
          {loadState}
          {loadError}
          onfilterchange={(key) => (activeFilter = key as FilterKey)}
          onselect={selectTask}
          onretry={retry}
          oncancel={cancel}
          ondelete={requestDelete}
          onnewtask={newTask}
          onrefresh={() => refresh()}
        />
      {:else if mode === "split" && selectedTask}
        <TasksDetailView
          {rows}
          totalCount={$tasks.length}
          task={selectedTask}
          {selectedTaskId}
          {chips}
          {activeFilter}
          {activeTab}
          {tabItems}
          {isRunning}
          onselect={selectTask}
          onfilterchange={(key) => (activeFilter = key as FilterKey)}
          onback={backToList}
          onnewtask={newTask}
          onrefresh={() => refresh()}
          oncancel={cancel}
          onretry={retry}
          ondelete={requestDelete}
          ontabchange={(key) => (activeTab = key as DetailTab)}
          onviewoutput={() => (activeTab = "output")}
        />
      {/if}
    </div>
  {/key}
</div>

<TasksFab onclick={newTask} />

<ConfirmDialog
  open={deleteTarget !== null}
  onclose={cancelDelete}
  onconfirm={confirmDelete}
  title={$t("tasks.delete_confirm_title")}
  message={$t("tasks.delete_confirm_message")}
  confirmLabel={$t("tasks.delete_confirm_yes")}
  cancelLabel={$t("common.cancel")}
  data-testid="task-delete-confirm"
/>
