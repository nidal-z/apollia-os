<script lang="ts">
  /**
   * ProjectTasksTab - tasks narrowed to the project's attached agents.
   *
   * Presentational: the parent maps `TaskSummary` to the operator `Task` row
   * shape and owns loading; this renders the list and the two empty states
   * (no agents attached vs. agents attached but no tasks yet).
   */
  import { t } from "svelte-i18n";
  import { Sparkles, Plus } from "lucide-svelte";
  import {
    SectionTitle,
    EmptyState,
    TaskRow,
    SkeletonList,
    type Task as RowTask,
  } from "$lib/components/operator";
  import { Button } from "$lib/components/ui/button";

  interface Props {
    rows: RowTask[];
    loading: boolean;
    /** Whether the project has at least one agent attached. */
    hasAgents: boolean;
    /** Jump to the Agents tab, the only place a link can be created. */
    onAttachAgents: () => void;
  }

  let { rows, loading, hasAgents, onAttachAgents }: Props = $props();
</script>

<SectionTitle count={rows.length}>
  {$t("projects.tab_tasks")}
</SectionTitle>
<div class="px-8 pb-6">
  {#if loading}
    <SkeletonList count={3} avatar rowClass="px-4 py-2" />
  {:else if !hasAgents}
    <EmptyState
      title={$t("projects.no_tasks")}
      desc={$t("projects.tasks_need_agents")}
    >
      {#snippet icon()}<Sparkles size={20} />{/snippet}
      {#snippet action()}
        <Button
          variant="primary-solid"
          size="sm"
          type="button"
          onclick={onAttachAgents}
          data-testid="project-tasks-attach-agents-btn"
        >
          {#snippet icon()}<Plus size={12} />{/snippet}
          {$t("projects.add_agent")}
        </Button>
      {/snippet}
    </EmptyState>
  {:else if rows.length === 0}
    <EmptyState
      title={$t("projects.no_tasks")}
      desc={$t("projects.no_tasks_desc")}
    >
      {#snippet icon()}<Sparkles size={20} />{/snippet}
    </EmptyState>
  {:else}
    <div class="border border-border rounded-xl overflow-hidden bg-card divide-y divide-border">
      {#each rows as task (task.id)}
        <TaskRow {task} />
      {/each}
    </div>
  {/if}
</div>
