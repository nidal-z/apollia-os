<script lang="ts">
  /**
   * TasksListView - the wide list surface for the Tasks route.
   *
   * Owns the PageHeader, the status FilterChipBar, and the list body across its
   * four states (loading skeleton, humanized load error, empty, populated).
   * Populated rows animate on add / remove / reorder (listMotion) and support
   * roving-tabindex keyboard navigation plus per-row context menus.
   */
  import { t } from "svelte-i18n";
  import { flip } from "svelte/animate";
  import { fly } from "svelte/transition";
  import { Plus, RefreshCw, ListTodo, Filter } from "lucide-svelte";
  import {
    PageHeader,
    FilterChipBar,
    ListPanel,
    EmptyState,
    SkeletonList,
    type FilterChip,
    type Task as DSTask,
  } from "$lib/components/operator";
  import { listNavigation } from "$lib/components/operator/listNavigation";
  import { Button } from "$lib/components/ui/button";
  import { listFlip, rowIn, rowOut } from "$lib/design/listMotion";
  import type { TaskSummary } from "$lib/types";
  import type { HumanizedError } from "$lib/errors/humanize";
  import TaskListRow from "./TaskListRow.svelte";
  import TasksLoadError from "./TasksLoadError.svelte";

  interface RowEntry {
    task: TaskSummary;
    row: DSTask;
  }

  interface Props {
    rows: RowEntry[];
    chips: FilterChip[];
    activeFilter: string;
    headlineTitle: string;
    headlineSubtitle: string;
    loadState: "loading" | "error" | "ready";
    loadError: HumanizedError | null;
    onfilterchange: (key: string) => void;
    onselect: (id: string) => void;
    onretry: (task: TaskSummary) => void;
    oncancel: (id: string) => void;
    ondelete: (task: TaskSummary) => void;
    onnewtask: () => void;
    onrefresh: () => void;
  }

  let {
    rows,
    chips,
    activeFilter,
    headlineTitle,
    headlineSubtitle,
    loadState,
    loadError,
    onfilterchange,
    onselect,
    onretry,
    oncancel,
    ondelete,
    onnewtask,
    onrefresh,
  }: Props = $props();

  const columns = $derived([
    { label: $t("tasks.col_task"), class: "flex-[2] min-w-0" },
    { label: $t("tasks.col_agent"), class: "w-[140px]" },
    { label: $t("tasks.col_progress"), class: "w-[140px]" },
    { label: $t("tasks.col_status"), class: "w-[120px]" },
    { label: $t("tasks.col_started"), class: "w-[90px] text-right" },
  ]);
</script>

<div class="mx-auto w-full max-w-6xl shrink-0">
  <PageHeader title={headlineTitle} subtitle={headlineSubtitle}>
    {#snippet actions()}
      <Button variant="outline" size="sm" onclick={onrefresh} data-testid="tasks-refresh">
        {#snippet icon()}<RefreshCw size={12} />{/snippet}
        {$t("common.refresh")}
      </Button>
      <Button variant="primary-solid" size="sm" onclick={onnewtask} data-testid="tasks-new">
        {#snippet icon()}<Plus size={12} />{/snippet}
        {$t("tasks.assign_task")}
      </Button>
    {/snippet}
  </PageHeader>
</div>

<div class="mx-auto w-full max-w-6xl">
  <FilterChipBar
    {chips}
    activeKey={activeFilter}
    onchange={onfilterchange}
    aria-label={$t("tasks.status_filters_aria")}
    testidPrefix="tasks-filter"
    data-testid="tasks-filter-bar"
  />

  <div class="px-8 pb-10">
    {#if loadState === "loading"}
      <ListPanel {columns} data-testid="tasks-loading">
        <div class="px-4 py-4">
          <SkeletonList count={5} avatar={false} />
        </div>
      </ListPanel>
    {:else if loadState === "error" && loadError}
      <ListPanel data-testid="tasks-error">
        <TasksLoadError error={loadError} onretry={onrefresh} />
      </ListPanel>
    {:else if rows.length === 0}
      <ListPanel data-testid="tasks-empty">
        <EmptyState
          title={activeFilter === "all"
            ? $t("tasks.empty_all_title")
            : $t("tasks.empty_filter_title")}
          desc={activeFilter === "all" ? $t("tasks.empty_all_desc") : $t("tasks.empty_filter_desc")}
        >
          {#snippet icon()}
            {#if activeFilter === "all"}<ListTodo size={22} />{:else}<Filter size={22} />{/if}
          {/snippet}
          {#snippet action()}
            {#if activeFilter === "all"}
              <Button variant="primary-solid" size="sm" onclick={onnewtask}>
                {#snippet icon()}<Plus size={12} />{/snippet}
                {$t("tasks.assign_task")}
              </Button>
            {:else}
              <Button variant="outline" size="sm" onclick={() => onfilterchange("all")}>
                {$t("tasks.view_all")}
              </Button>
            {/if}
          {/snippet}
        </EmptyState>
      </ListPanel>
    {:else}
      <div use:listNavigation={{ idPrefix: "task-row" }} data-testid="tasks-list-nav">
        <ListPanel {columns} data-testid="tasks-table">
          {#each rows as entry (entry.task.id)}
            <div animate:flip={listFlip()} in:fly={rowIn()} out:fly={rowOut()}>
              <TaskListRow
                task={entry.task}
                row={entry.row}
                {onselect}
                {onretry}
                {oncancel}
                {ondelete}
              />
            </div>
          {/each}
        </ListPanel>
      </div>
    {/if}
  </div>
</div>
