<script lang="ts">
  /**
   * WorkflowRunRow - one run in the history of a workflow: when it started,
   * how long it took, its inputs on one line and its outcome. The row opens
   * the steps of that run.
   */
  import { t, locale } from "svelte-i18n";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import { formatElapsedClock, formatRelativeTime } from "$lib/utils";
  import type { WorkflowRun } from "$lib/ipc/workflows";
  import { elapsedMs, inputsSummary, runLabelKey, runTone } from "./workflowRuns";

  interface Props {
    run: WorkflowRun;
    /** Whether its steps are the ones shown. */
    selected: boolean;
    now: number;
    onopen: () => void;
  }

  let { run, selected, now, onopen }: Props = $props();

  const elapsed = $derived(elapsedMs(run.startedAt, run.endedAt, now));
  const summary = $derived(inputsSummary(run.inputs));
</script>

<li data-testid="workflow-run" data-run-id={run.runId} data-status={run.status} data-selected={selected}>
  <Button
    variant="ghost"
    size="auto"
    onclick={onopen}
    aria-pressed={selected}
    class="flex w-full items-center gap-3 rounded-md border px-3 py-2 text-left {selected
      ? 'border-primary/40 bg-primary/10'
      : 'border-border/60 hover:bg-surface-1'}"
    data-testid="workflow-run-open"
  >
    <span class="min-w-0 flex-1">
      <span class="block text-body-xs text-foreground">
        {formatRelativeTime(run.startedAt, $locale ?? "fr")}
        {#if elapsed !== null}
          <span class="text-muted-foreground"> · {formatElapsedClock(Math.floor(elapsed / 1000))}</span>
        {/if}
      </span>
      <span class="block truncate text-caption text-muted-foreground">
        {summary || $t("workflows.run_no_inputs")}
      </span>
    </span>
    <Badge size="sm" variant={runTone(run.status)} class="shrink-0">{$t(runLabelKey(run.status))}</Badge>
  </Button>
</li>
