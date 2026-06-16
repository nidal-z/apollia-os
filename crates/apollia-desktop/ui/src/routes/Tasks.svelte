<script lang="ts">
  import { onMount } from "svelte";
  import { t } from "svelte-i18n";
  import { invoke } from "@tauri-apps/api/core";
  import {
    Plus,
    RefreshCw,
    ListTodo,
    Activity,
    CheckCircle,
    XCircle,
    Clock,
    AlertTriangle,
    Ban,
    Timer,
    Calendar,
  } from "lucide-svelte";
  import type { TaskSummary } from "$lib/types";
  import { tasks, openNewTaskRequested } from "$lib/stores/tasks";
  import { uiMode } from "$lib/stores/mode";
  import { formatRelativeTime } from "$lib/utils";
  import {
    PageHeader,
    DetailHeader,
    SidebarHeader,
    ListRow,
    EmptyState,
    TaskRow,
    ListPanel,
    FilterChipBar,
    SplitLayout,
    type Task as DSTask,
    type TaskStatus as DSTaskStatus,
  } from "$lib/components/operator";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import { Avatar } from "$lib/components/ui/avatar";
  import { Card } from "$lib/components/ui/card";
  import { Separator } from "$lib/components/ui/separator";
  import { TabBar } from "$lib/components/ui/tabs";
  import { BuilderOnly } from "$lib/components/shared";
  import ExecutionTrace from "../components/observability/ExecutionTrace.svelte";
  import SmartOutput from "../components/common/SmartOutput.svelte";
  import TasksFab from "../components/tasks/TasksFab.svelte";

  // ─── Selection / mode ─────────────────────────────────────────────────────
  type Mode = "list" | "split";
  type DetailTab = "overview" | "output" | "trace" | "technical";

  let mode = $state<Mode>("list");
  let selectedTaskId = $state<string | null>(null);
  let activeTab = $state<DetailTab>("overview");

  function handleSelectTask(taskId: string) {
    selectedTaskId = taskId;
    mode = "split";
    activeTab = "overview";
  }

  function backToList() {
    mode = "list";
    selectedTaskId = null;
  }

  function handleNewTask() {
    openNewTaskRequested.set(Date.now());
  }

  async function handleRefresh() {
    try {
      const result: TaskSummary[] = await invoke("list_tasks", { filter: null });
      tasks.set(result);
    } catch {
      // runtime not ready - keep current state
    }
  }

  // ─── Status filter chips ───────────────────────────────────────────────────
  type FilterKey = "all" | "todo" | "running" | "approval" | "done" | "error";

  let activeFilter = $state<FilterKey>("all");

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

  const filteredTasks = $derived.by(() => {
    const filter = FILTERS.find((f) => f.key === activeFilter)!;
    return $tasks.filter((task) => filter.match(task.status));
  });

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

  // ─── TaskSummary → DS Task adapter ────────────────────────────────────────
  const STATUS_MAP: Record<TaskSummary["status"], DSTaskStatus> = {
    submitted: "queued",
    working: "running",
    completed: "completed",
    failed: "failed",
    input_required: "awaiting_approval",
    canceled: "cancelled",
  };

  function toDSTask(task: TaskSummary): DSTask {
    const progress =
      task.status === "completed"
        ? 100
        : task.status === "working"
          ? 45
          : task.status === "input_required"
            ? 20
            : task.status === "failed"
              ? 60
              : 0;
    return {
      id: task.id,
      title: task.output_text || task.input_preview || ($t("tasks.untitled") as string),
      agent: task.agent_name || task.agent_id,
      status: STATUS_MAP[task.status] ?? "queued",
      progress,
      started: formatRelativeTime(task.created_at),
    };
  }

  // ─── Detail view state ─────────────────────────────────────────────────────
  const TRUNCATION_MARKER = "[TRONQUE]";

  const STATUS_BADGE: Record<string, { variant: "success" | "destructive" | "warning" | "info" | "secondary"; icon: typeof Activity }> = {
    completed: { variant: "success", icon: CheckCircle },
    working: { variant: "info", icon: Activity },
    submitted: { variant: "secondary", icon: Clock },
    failed: { variant: "destructive", icon: XCircle },
    input_required: { variant: "warning", icon: AlertTriangle },
    canceled: { variant: "secondary", icon: Ban },
  };

  const STATUS_I18N: Record<string, string> = {
    working: "dashboard.status_working",
    submitted: "dashboard.status_submitted",
    completed: "dashboard.status_completed",
    failed: "dashboard.status_failed",
    input_required: "dashboard.status_approval",
    canceled: "dashboard.status_canceled",
  };

  const selectedTask = $derived($tasks.find((t) => t.id === selectedTaskId));
  const isRunning = $derived(
    selectedTask?.status === "working" || selectedTask?.status === "submitted",
  );
  const inputTruncated = $derived(
    selectedTask?.input_preview?.includes(TRUNCATION_MARKER) ?? false,
  );
  const outputTruncated = $derived(
    selectedTask?.output_text?.includes(TRUNCATION_MARKER) ?? false,
  );

  const detailTabItems = $derived.by(() => {
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

  function formatDurationLong(ms: number | undefined): string {
    if (ms === undefined || ms === null) return "-";
    if (ms < 1000) return `${ms}ms`;
    if (ms < 60_000) return `${(ms / 1000).toFixed(1)}s`;
    const mins = Math.floor(ms / 60_000);
    const secs = Math.floor((ms % 60_000) / 1000);
    return `${mins}m ${secs}s`;
  }

  function formatDate(iso: string): string {
    if (!iso) return "-";
    return new Date(iso).toLocaleString();
  }

  function taskShortTitle(task: TaskSummary): string {
    const raw = task.input_preview || task.output_text || ($t("tasks.untitled") as string);
    return raw.length > 80 ? raw.slice(0, 80) + "…" : raw;
  }

  onMount(() => {
    // Page mount - store is already populated by sse.ts watchdog/refresh.
  });
</script>

<div class="flex h-full min-h-0 w-full flex-col" data-testid="tasks-page">
  {#if mode === "list"}
    <!-- ============ LIST (all tasks, wide table) ============ -->
    <div class="mx-auto w-full max-w-6xl shrink-0">
      <PageHeader        title={headlineTitle}
        subtitle={headlineSubtitle}
      >
        {#snippet actions()}
          <Button variant="outline" size="sm" onclick={handleRefresh}>
            {#snippet icon()}<RefreshCw size={12} />{/snippet}
            {$t("common.refresh")}
          </Button>
          <Button variant="primary-solid" size="sm" onclick={handleNewTask}>
            {#snippet icon()}<Plus size={12} />{/snippet}
            {$t("tasks.assign_task")}
          </Button>
        {/snippet}
      </PageHeader>
    </div>
    <div class="mx-auto w-full max-w-6xl">
      <!-- Status filter chip row -->
      <FilterChipBar
        chips={FILTERS.map((f) => ({
          key: f.key,
          label: $t(f.labelKey),
          color: f.color,
          count: counts[f.key],
          glowWhenActive: f.key === "running",
        }))}
        activeKey={activeFilter}
        onchange={(key) => (activeFilter = key as FilterKey)}
        aria-label={$t("tasks.status_filters_aria")}
        testidPrefix="tasks-filter"
        data-testid="tasks-filter-bar"
      />

      <div class="px-8 pb-10">
        {#if filteredTasks.length === 0}
          <ListPanel data-testid="tasks-empty">
            <EmptyState
              title={activeFilter === "all" ? $t("tasks.empty_all_title") : $t("tasks.empty_filter_title")}
              desc={activeFilter === "all"
                ? $t("tasks.empty_all_desc")
                : $t("tasks.empty_filter_desc")}
            >
              {#snippet icon()}<ListTodo size={22} />{/snippet}
              {#snippet action()}
                {#if activeFilter === "all"}
                  <Button variant="primary-solid" size="sm" onclick={handleNewTask}>
                    {#snippet icon()}<Plus size={12} />{/snippet}
                    {$t("tasks.assign_task")}
                  </Button>
                {:else}
                  <Button variant="outline" size="sm" onclick={() => (activeFilter = "all")}>
                    {$t("tasks.view_all")}
                  </Button>
                {/if}
              {/snippet}
            </EmptyState>
          </ListPanel>
        {:else}
          <ListPanel
            data-testid="tasks-table"
            columns={[
              { label: $t("tasks.col_task"), class: "flex-[2] min-w-0" },
              { label: $t("tasks.col_agent"), class: "w-[140px]" },
              { label: $t("tasks.col_progress"), class: "w-[140px]" },
              { label: $t("tasks.col_status"), class: "w-[120px]" },
              { label: $t("tasks.col_started"), class: "w-[90px] text-right" },
            ]}
          >
            {#each filteredTasks as task (task.id)}
              <TaskRow
                task={toDSTask(task)}
                onclick={() => handleSelectTask(task.id)}
              />
            {/each}
          </ListPanel>
        {/if}
      </div>
    </div>
  {:else if mode === "split" && selectedTask}
    <!-- ============ SPLIT (detail view, mirror Projets) ============ -->
    {@const cfg = STATUS_BADGE[selectedTask.status] ?? STATUS_BADGE.submitted}
    {@const StatusIcon = cfg.icon}
    <SplitLayout sidebarTestid="tasks-split-sidebar" detailTestid="task-detail">
      {#snippet sidebar()}
        <!-- LEFT: task list -->
        <SidebarHeader title={$t("tasks.sidebar_heading")} count={$tasks.length}>
          {#snippet filters()}
            <FilterChipBar
              size="compact"
              chips={FILTERS.map((f) => ({
                key: f.key,
                label: $t(f.labelKey),
                color: f.color,
                count: counts[f.key],
                glowWhenActive: f.key === "running",
              }))}
              activeKey={activeFilter}
              onchange={(key) => (activeFilter = key as FilterKey)}
              aria-label={$t("tasks.status_filters_aria")}
              testidPrefix="tasks-split-filter"
            />
          {/snippet}
        </SidebarHeader>
        <div class="flex-1 overflow-auto px-2.5 pb-3" data-testid="tasks-split-list">
          {#each filteredTasks as task (task.id)}
            {@const isActive = task.id === selectedTaskId}
            {@const rowCfg = STATUS_BADGE[task.status] ?? STATUS_BADGE.submitted}
            <ListRow
              variant="nav"
              state={isActive ? "active" : "default"}
              class="mb-0.5 text-left"
              onclick={() => handleSelectTask(task.id)}
              data-testid="tasks-split-row-{task.id}"
            >
              <div
                class="w-0.5 self-stretch rounded-sm shrink-0 my-0.5"
                style="background: {task.status === 'completed'
                  ? 'hsl(var(--success))'
                  : task.status === 'failed'
                    ? 'hsl(var(--destructive))'
                    : task.status === 'working'
                      ? 'hsl(var(--primary))'
                      : task.status === 'input_required'
                        ? 'hsl(var(--warning))'
                        : 'hsl(var(--muted-foreground))'};"
              ></div>
              <div class="min-w-0 flex-1">
                <div
                  class="text-[12.5px] truncate text-foreground"
                  style:font-weight={isActive ? 600 : 500}
                >
                  {taskShortTitle(task)}
                </div>
                <div class="text-[10.5px] text-muted-foreground mt-0.5 truncate flex items-center gap-1">
                  <span>{task.agent_name || task.agent_id}</span>
                  <span class="opacity-60">·</span>
                  <span>{formatRelativeTime(task.created_at)}</span>
                </div>
              </div>
              <Badge size="sm" variant={rowCfg.variant} class="shrink-0 text-[8px] px-1 py-0 leading-[1.4]">
                {$t(STATUS_I18N[task.status] ?? "dashboard.status_submitted")}
              </Badge>
            </ListRow>
          {/each}
        </div>
        <div class="px-3 py-2 border-t border-border">
          <Button variant="outline" size="sm" onclick={backToList} data-testid="tasks-back-to-list">
            ← {$t("common.back")}
          </Button>
        </div>
      {/snippet}

      <!-- RIGHT: detail -->
        {@const task = selectedTask}
        <!-- Detail header (screen identity is the breadcrumb) -->
        <DetailHeader
          title={task.agent_name || task.agent_id}
          titleTestid="task-detail-title"
        >
          {#snippet leading()}
            <Avatar
              name={task.agent_name || task.agent_id}
              fallback={(task.agent_name || "?").charAt(0).toUpperCase()}
              size="lg"
              ring
            />
          {/snippet}
          {#snippet badges()}
            <Badge variant={cfg.variant} size="sm">
              {#snippet icon()}
                <StatusIcon size={11} />
              {/snippet}
              {$t(STATUS_I18N[task.status] ?? "dashboard.status_submitted")}
            </Badge>
            <BuilderOnly>
              <code class="text-[10px] text-muted-foreground/40 font-mono">
                {task.id.slice(0, 12)}
              </code>
            </BuilderOnly>
          {/snippet}
          {#snippet footer()}
            <span class="inline-flex items-center gap-1 text-[11px] text-muted-foreground">
              <Timer size={11} />
              {formatDurationLong(task.duration_ms)}
            </span>
            <span class="inline-flex items-center gap-1 text-[11px] text-muted-foreground">
              <Calendar size={11} />
              {formatDate(task.created_at)}
            </span>
          {/snippet}
          {#snippet actions()}
            <Button variant="outline" size="sm" onclick={handleRefresh}>
              {#snippet icon()}<RefreshCw size={12} />{/snippet}
              {$t("common.refresh")}
            </Button>
            <Button variant="primary-solid" size="sm" onclick={handleNewTask}>
              {#snippet icon()}<Plus size={12} />{/snippet}
              {$t("tasks.assign_task")}
            </Button>
          {/snippet}
        </DetailHeader>

        <!-- Tabs -->
        <div class="px-8 pt-3.5">
          <TabBar
            variant="underline"
            testidPrefix="task-detail"
            activeTab={activeTab}
            items={detailTabItems}
            ontabchange={(key) => (activeTab = key as DetailTab)}
          />
        </div>

        <!-- Tab content -->
        <div class="flex-1 overflow-auto px-8 py-5">
          {#if activeTab === "overview"}
            <div class="space-y-3 max-w-3xl">
              <!-- Input card -->
              <Card class="rounded-lg px-4 py-3.5" data-testid="task-overview-input">
                <div class="flex items-center gap-2 mb-2">
                  <span class="text-[10px] font-medium uppercase tracking-wider text-muted-foreground/50">
                    {$t('tasks.input')}
                  </span>
                  {#if inputTruncated}
                    <Badge variant="outline" class="text-[9px] px-1 py-0">{$t('tasks.truncated')}</Badge>
                  {/if}
                </div>
                <p class="whitespace-pre-wrap text-xs text-foreground/85 leading-relaxed">
                  {selectedTask.input_preview || $t('common.no_input')}
                </p>
              </Card>

              <!-- Output / error preview card -->
              {#if selectedTask.output_text}
                <Card
                  class="rounded-lg px-4 py-3.5 {selectedTask.status === 'failed' ? 'border-destructive/30' : ''}"
                  data-testid="task-overview-output"
                >
                  <div class="flex items-center gap-2 mb-2">
                    {#if selectedTask.status === 'failed'}
                      <XCircle size={11} class="text-destructive shrink-0" />
                      <span class="text-[10px] font-medium uppercase tracking-wider text-destructive/70">
                        {$t('tasks.error')}
                      </span>
                    {:else}
                      <span class="text-[10px] font-medium uppercase tracking-wider text-muted-foreground/50">
                        {$t('tasks.result')}
                      </span>
                    {/if}
                    {#if outputTruncated}
                      <Badge variant="outline" class="text-[9px] px-1 py-0">{$t('tasks.truncated')}</Badge>
                    {/if}
                  </div>
                  <div class="text-xs {selectedTask.status === 'failed' ? 'text-destructive/80 font-mono break-all' : 'text-foreground/85 leading-relaxed'}">
                    {#if selectedTask.status === 'failed'}
                      {selectedTask.output_text}
                    {:else}
                      <p class="whitespace-pre-wrap">
                        {selectedTask.output_text.length > 280
                          ? selectedTask.output_text.slice(0, 280) + "…"
                          : selectedTask.output_text}
                      </p>
                      {#if selectedTask.output_text.length > 280}
                        <button
                          type="button"
                          class="mt-1.5 text-[11px] text-primary hover:underline"
                          onclick={() => (activeTab = "output")}
                        >
                          {$t("tasks.view_full_output")} →
                        </button>
                      {/if}
                    {/if}
                  </div>
                </Card>
              {/if}
            </div>
          {:else if activeTab === "output"}
            <div class="max-w-3xl" data-testid="task-output-tab">
              {#if selectedTask.output_text}
                {#if selectedTask.status === "failed"}
                  <Card class="rounded-lg px-4 py-3.5 border-destructive/30">
                    <div class="flex items-center gap-2 mb-2">
                      <XCircle size={11} class="text-destructive shrink-0" />
                      <span class="text-[10px] font-medium uppercase tracking-wider text-destructive/70">
                        {$t('tasks.error')}
                      </span>
                    </div>
                    <pre class="whitespace-pre-wrap break-all text-xs text-destructive/80 font-mono">{selectedTask.output_text}</pre>
                  </Card>
                {:else}
                  <SmartOutput output={selectedTask.output_text} />
                {/if}
              {:else}
                <p class="text-[12.5px] text-muted-foreground italic">{$t("tasks.no_output")}</p>
              {/if}
            </div>
          {:else if activeTab === "trace"}
            <div data-testid="task-trace-tab">
              <ExecutionTrace
                taskId={selectedTask.id}
                context="task"
                mode={isRunning ? "live" : "replay"}
              />
            </div>
          {:else if activeTab === "technical"}
            <div class="max-w-3xl space-y-3" data-testid="task-technical-tab">
              <Card class="rounded-lg px-4 py-3.5">
                <div class="text-[10px] font-medium uppercase tracking-wider text-muted-foreground/50 mb-2">
                  {$t("tasks.tech_identifiers")}
                </div>
                <dl class="space-y-1.5 text-xs">
                  <div class="flex items-baseline gap-3">
                    <dt class="w-[120px] shrink-0 text-muted-foreground">{$t("tasks.tech_task_id")}</dt>
                    <dd><code class="text-[11px] font-mono text-foreground/85 break-all">{selectedTask.id}</code></dd>
                  </div>
                  <div class="flex items-baseline gap-3">
                    <dt class="w-[120px] shrink-0 text-muted-foreground">{$t("tasks.tech_agent_id")}</dt>
                    <dd><code class="text-[11px] font-mono text-foreground/85">{selectedTask.agent_id}</code></dd>
                  </div>
                  <div class="flex items-baseline gap-3">
                    <dt class="w-[120px] shrink-0 text-muted-foreground">{$t("tasks.tech_agent_name")}</dt>
                    <dd class="text-foreground/85">{selectedTask.agent_name || "-"}</dd>
                  </div>
                </dl>
              </Card>

              <Card class="rounded-lg px-4 py-3.5">
                <div class="text-[10px] font-medium uppercase tracking-wider text-muted-foreground/50 mb-2">
                  {$t("tasks.tech_status_timing")}
                </div>
                <Separator class="mb-2.5" />
                <dl class="space-y-1.5 text-xs">
                  <div class="flex items-baseline gap-3">
                    <dt class="w-[120px] shrink-0 text-muted-foreground">{$t("tasks.tech_raw_status")}</dt>
                    <dd><code class="text-[11px] font-mono text-foreground/85">{selectedTask.status}</code></dd>
                  </div>
                  <div class="flex items-baseline gap-3">
                    <dt class="w-[120px] shrink-0 text-muted-foreground">{$t("tasks.created")}</dt>
                    <dd class="text-foreground/85">{formatDate(selectedTask.created_at)}</dd>
                  </div>
                  <div class="flex items-baseline gap-3">
                    <dt class="w-[120px] shrink-0 text-muted-foreground">{$t("tasks.duration")}</dt>
                    <dd class="text-foreground/85 tabular-nums">{formatDurationLong(selectedTask.duration_ms)}</dd>
                  </div>
                </dl>
              </Card>
            </div>
          {/if}
        </div>
    </SplitLayout>
  {/if}
</div>

<!-- Mobile FAB (operator). Desktop uses the top-right button in the header. -->
<TasksFab onclick={handleNewTask} />
