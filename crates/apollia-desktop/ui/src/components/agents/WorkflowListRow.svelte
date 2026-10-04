<script lang="ts">
  /**
   * WorkflowListRow - one workflow in the Workflows column of the screen:
   * its name, what it does, and its last run. The row opens the sheet;
   * "Launch" opens it on the launch form. While a run is going, the row says
   * so and leads to its steps.
   */
  import { t, locale } from "svelte-i18n";
  import { Play, Workflow } from "lucide-svelte";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import { formatRelativeTime } from "$lib/utils";
  import type { WorkflowListItem, WorkflowRun } from "$lib/ipc/workflows";
  import { isRunning, runLabelKey } from "../workflows/workflowRuns";

  interface Props {
    workflow: WorkflowListItem;
    /** Its most recent run, when it has one. */
    lastRun: WorkflowRun | null;
    onOpen: () => void;
    onLaunch: () => void;
  }

  let { workflow, lastRun, onOpen, onLaunch }: Props = $props();

  const going = $derived(lastRun !== null && isRunning(lastRun));
</script>

<div
  role="option"
  aria-selected="false"
  class="group mb-0.5 flex w-full items-center gap-2 rounded-lg pr-1 transition-colors hover:bg-surface-1"
  data-testid="workflow-row"
  data-workflow={workflow.workflowId}
  data-running={going}
>
  <Button
    variant="ghost"
    size="auto"
    type="button"
    data-nav-row
    onclick={onOpen}
    class="flex min-w-0 flex-1 items-center gap-2.5 rounded-lg px-2.5 py-2 text-left"
    data-testid="workflow-list-row"
  >
    <span
      class="inline-flex h-7 w-7 shrink-0 items-center justify-center rounded-lg border border-border bg-surface-1 text-primary"
    >
      <Workflow size={13} />
    </span>
    <span class="min-w-0 flex-1">
      <span class="block truncate text-body-xs font-medium text-foreground">{workflow.name}</span>
      <span class="block truncate text-caption text-muted-foreground">
        {workflow.description || workflow.workflowId}
      </span>
      {#if lastRun && !going}
        <span class="block truncate text-caption text-muted-foreground/80" data-testid="workflow-row-last-run">
          {$t("workflows.meta_last_run", {
            values: {
              when: formatRelativeTime(lastRun.startedAt, $locale ?? "fr"),
              outcome: $t(runLabelKey(lastRun.status)).toLowerCase(),
            },
          })}
        </span>
      {/if}
    </span>
  </Button>
  {#if going}
    <Badge size="sm" variant="info" class="shrink-0">{$t("workflows.run_running")}</Badge>
    <Button variant="outline" size="sm" class="shrink-0" onclick={onOpen} data-testid="workflow-row-steps">
      {$t("workflows.see_steps")}
    </Button>
  {:else}
    <Button variant="outline" size="sm" class="shrink-0" onclick={onLaunch} data-testid="workflow-row-launch">
      {#snippet icon()}<Play size={11} />{/snippet}
      {$t("workflows.launch")}
    </Button>
  {/if}
</div>
