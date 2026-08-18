<script lang="ts">
  /**
   * TasksDetailView - the master/detail (split) surface for a selected task.
   *
   * Left: a filtered, navigable task list (TaskNavRow). Right: the DetailHeader
   * with status-aware actions (Stop a running task, Retry a failed one), the
   * tab bar, and the tab panes. The selected task is the expressive focal point,
   * carried by the ringed avatar and the live overview card in the panes.
   */
  import { t, locale } from "svelte-i18n";
  import { Plus, RefreshCw, Pause, Timer, Calendar, Trash2 } from "lucide-svelte";
  import {
    DetailHeader,
    SidebarHeader,
    SplitLayout,
    FilterChipBar,
    type FilterChip,
    type Task as DSTask,
  } from "$lib/components/operator";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import { Avatar } from "$lib/components/ui/avatar";
  import { TabBar } from "$lib/components/ui/tabs";
  import { BuilderOnly } from "$lib/components/shared";
  import type { TaskSummary } from "$lib/types";
  import { statusMeta } from "./taskStatus";
  import { formatDate } from "./taskFormat";
  import { formatDuration } from "$lib/utils";
  import TaskNavRow from "./TaskNavRow.svelte";
  import TasksDetailPanes from "./TasksDetailPanes.svelte";

  interface RowEntry {
    task: TaskSummary;
    row: DSTask;
  }

  interface Props {
    rows: RowEntry[];
    totalCount: number;
    task: TaskSummary;
    selectedTaskId: string | null;
    chips: FilterChip[];
    activeFilter: string;
    activeTab: string;
    tabItems: { key: string; label: string }[];
    isRunning: boolean;
    onselect: (id: string) => void;
    onfilterchange: (key: string) => void;
    onback: () => void;
    onnewtask: () => void;
    onrefresh: () => void;
    oncancel: (id: string) => void;
    onretry: (task: TaskSummary) => void;
    ondelete: (task: TaskSummary) => void;
    ontabchange: (key: string) => void;
    onviewoutput: () => void;
  }

  let {
    rows,
    totalCount,
    task,
    selectedTaskId,
    chips,
    activeFilter,
    activeTab,
    tabItems,
    isRunning,
    onselect,
    onfilterchange,
    onback,
    onnewtask,
    onrefresh,
    oncancel,
    onretry,
    ondelete,
    ontabchange,
    onviewoutput,
  }: Props = $props();

  const meta = $derived(statusMeta(task.status));
  const StatusIcon = $derived(meta.icon);
  const agentLabel = $derived(task.agent_name || task.agent_id);
  const isFailed = $derived(task.status === "failed");
  // Delete erases a settled record; a live task must be stopped, not deleted.
  const isDeletable = $derived(!isRunning && task.status !== "input_required");
</script>

<SplitLayout sidebarTestid="tasks-split-sidebar" detailTestid="task-detail">
  {#snippet sidebar()}
    <SidebarHeader title={$t("tasks.sidebar_heading")} count={totalCount}>
      {#snippet filters()}
        <FilterChipBar
          size="compact"
          {chips}
          activeKey={activeFilter}
          onchange={onfilterchange}
          aria-label={$t("tasks.status_filters_aria")}
          testidPrefix="tasks-split-filter"
        />
      {/snippet}
    </SidebarHeader>
    <div class="flex-1 overflow-auto px-2.5 pb-3" data-testid="tasks-split-list">
      {#each rows as entry (entry.task.id)}
        <TaskNavRow
          task={entry.task}
          title={entry.row.title}
          active={entry.task.id === selectedTaskId}
          {onselect}
        />
      {/each}
    </div>
    <div class="px-3 py-2 border-t border-border">
      <Button variant="outline" size="sm" onclick={onback} data-testid="tasks-back-to-list">
        ← {$t("common.back")}
      </Button>
    </div>
  {/snippet}

  <DetailHeader title={agentLabel} titleTestid="task-detail-title">
    {#snippet leading()}
      <Avatar
        name={agentLabel}
        fallback={(task.agent_name || "?").charAt(0).toUpperCase()}
        size="lg"
        ring
      />
    {/snippet}
    {#snippet badges()}
      <Badge variant={meta.variant} size="sm">
        {#snippet icon()}<StatusIcon size={11} />{/snippet}
        {$t(meta.i18nKey)}
      </Badge>
      <BuilderOnly>
        <code class="text-body-xs text-muted-foreground/40 font-mono">{task.id.slice(0, 12)}</code>
      </BuilderOnly>
    {/snippet}
    {#snippet footer()}
      <span class="inline-flex items-center gap-1 text-body-xs text-muted-foreground">
        <Timer size={11} />
        {formatDuration(task.duration_ms)}
      </span>
      <span class="inline-flex items-center gap-1 text-body-xs text-muted-foreground">
        <Calendar size={11} />
        {formatDate(task.created_at, $locale ?? "en")}
      </span>
    {/snippet}
    {#snippet actions()}
      {#if isRunning}
        <Button variant="outline" size="sm" onclick={() => oncancel(task.id)} data-testid="task-stop">
          {#snippet icon()}<Pause size={11} />{/snippet}
          {$t("tasks.stop")}
        </Button>
      {:else if isFailed}
        <Button variant="primary-solid" size="sm" onclick={() => onretry(task)} data-testid="task-retry">
          {#snippet icon()}<RefreshCw size={11} />{/snippet}
          {$t("tasks.retry")}
        </Button>
      {/if}
      {#if isDeletable}
        <Button
          variant="destructive"
          size="sm"
          onclick={() => ondelete(task)}
          data-testid="task-delete"
        >
          {#snippet icon()}<Trash2 size={11} />{/snippet}
          {$t("tasks.delete")}
        </Button>
      {/if}
      <Button variant="outline" size="sm" onclick={onrefresh}>
        {#snippet icon()}<RefreshCw size={12} />{/snippet}
        {$t("common.refresh")}
      </Button>
      <Button variant="primary-solid" size="sm" onclick={onnewtask}>
        {#snippet icon()}<Plus size={12} />{/snippet}
        {$t("tasks.assign_task")}
      </Button>
    {/snippet}
  </DetailHeader>

  <div class="px-8 pt-3.5">
    <TabBar
      variant="underline"
      testidPrefix="task-detail"
      {activeTab}
      items={tabItems}
      ontabchange={ontabchange}
    />
  </div>

  <div class="flex-1 overflow-auto px-8 py-5">
    <TasksDetailPanes {task} {activeTab} {isRunning} {onviewoutput} />
  </div>
</SplitLayout>
