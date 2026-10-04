<script lang="ts">
  /**
   * WorkflowRunCard - the run shown at the top of the sheet. Going, it says
   * when it started, which step it is at, how long it has been running and
   * what it was launched with, and it can be stopped, after a confirmation.
   * Ended, it says how, and offers to launch again with the same inputs.
   */
  import { t, locale } from "svelte-i18n";
  import { RotateCcw, Square, X } from "lucide-svelte";
  import { Card } from "$lib/components/operator";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import ConfirmDialog from "$lib/components/ui/dialog/ConfirmDialog.svelte";
  import { ProgressBar } from "$lib/components/ui/progress";
  import { developerMode } from "$lib/stores/developerMode";
  import { formatElapsedClock, formatRelativeTime } from "$lib/utils";
  import type { WorkflowRunDetail } from "$lib/ipc/workflows";
  import {
    elapsedMs,
    inputsSummary,
    isRunning,
    progressOf,
    runLabelKey,
    runTone,
    stepPosition,
  } from "./workflowRuns";

  interface Props {
    detail: WorkflowRunDetail;
    now: number;
    stopping: boolean;
    onstop: () => void;
    /** Opens the launch form filled with the inputs of this run. */
    onrelaunch: () => void;
    onclose: () => void;
  }

  let { detail, now, stopping, onstop, onrelaunch, onclose }: Props = $props();

  const run = $derived(detail.run);
  const going = $derived(isRunning(run));
  const position = $derived(stepPosition(detail.steps));
  const elapsed = $derived(elapsedMs(run.startedAt, run.endedAt, now));
  const summary = $derived(inputsSummary(run.inputs));

  let confirming = $state(false);
</script>

<div data-testid="workflow-run-card" data-status={run.status} data-run-id={run.runId}>
<Card class="space-y-3 p-4">
  <div class="flex flex-wrap items-start justify-between gap-3">
    <div class="min-w-0">
      <div class="flex flex-wrap items-center gap-2">
        <h3 class="m-0 text-body-sm font-semibold text-foreground">{$t("workflows.run_card_title")}</h3>
        <Badge size="sm" variant={runTone(run.status)} data-testid="workflow-run-card-status">
          {$t(runLabelKey(run.status))}
        </Badge>
      </div>
      <p class="m-0 mt-1 text-caption text-muted-foreground" data-testid="workflow-run-card-meta">
        {$t("workflows.run_started", { values: { when: formatRelativeTime(run.startedAt, $locale ?? "fr") } })}
        {#if position.total > 0}
          · {$t("workflows.run_step_position", { values: { current: position.current, total: position.total } })}
        {/if}
        {#if elapsed !== null}
          · {formatElapsedClock(Math.floor(elapsed / 1000))}
        {/if}
      </p>
    </div>
    <div class="flex shrink-0 flex-wrap items-center gap-2">
      {#if going}
        <Button
          variant="destructive"
          size="sm"
          disabled={stopping}
          onclick={() => (confirming = true)}
          data-testid="workflow-run-stop"
        >
          {#snippet icon()}<Square size={11} />{/snippet}
          {$t("workflows.stop")}
        </Button>
      {:else}
        <Button variant="outline" size="sm" onclick={onrelaunch} data-testid="workflow-run-relaunch">
          {#snippet icon()}<RotateCcw size={11} />{/snippet}
          {$t("workflows.relaunch_same")}
        </Button>
        <Button
          variant="ghost"
          size="icon-sm"
          onclick={onclose}
          aria-label={$t("workflows.run_close")}
          title={$t("workflows.run_close")}
          data-testid="workflow-run-close"
        >
          <X size={13} />
        </Button>
      {/if}
    </div>
  </div>

  {#if going && position.total > 0}
    <ProgressBar value={progressOf(detail.steps)} size="sm" label={$t("workflows.run_progress")} />
  {/if}

  {#if run.status === "canceled"}
    <p class="m-0 text-body-xs text-foreground/85" data-testid="workflow-run-card-canceled">
      {$t("workflows.run_canceled_note")}
    </p>
  {/if}
  {#if run.error && run.status === "failed"}
    <p class="m-0 whitespace-pre-wrap break-words text-body-xs text-destructive" data-testid="workflow-run-card-error">
      {run.error}
    </p>
  {/if}

  <p class="m-0 text-caption text-muted-foreground" data-testid="workflow-run-card-inputs">
    <span class="font-semibold">{$t("workflows.run_inputs")}</span>
    {summary || $t("workflows.run_no_inputs")}
  </p>

  {#if $developerMode}
    <div class="space-y-1 border-t border-border/60 pt-3" data-testid="workflow-run-technical">
      <div class="text-caption font-semibold text-muted-foreground">{$t("dev.workflows.technical_title")}</div>
      <dl class="m-0 grid grid-cols-[auto_1fr] gap-x-3 gap-y-0.5 font-mono text-caption text-muted-foreground">
        <dt>{$t("dev.workflows.run_id")}</dt>
        <dd class="m-0 break-all">{run.runId}</dd>
        <dt>{$t("dev.workflows.workflow_id")}</dt>
        <dd class="m-0 break-all">{run.workflowId}</dd>
        <dt>{$t("dev.workflows.depth")}</dt>
        <dd class="m-0">{run.depth}</dd>
        {#if run.parentRunId}
          <dt>{$t("dev.workflows.parent_run")}</dt>
          <dd class="m-0 break-all">{run.parentRunId}</dd>
        {/if}
      </dl>
      <pre class="m-0 max-h-40 overflow-auto whitespace-pre-wrap break-all rounded-md bg-surface-1 p-2 font-mono text-caption text-foreground/85">{JSON.stringify(
          { inputs: run.inputs, output: run.output ?? null, error: run.error ?? null },
          null,
          2,
        )}</pre>
    </div>
  {/if}
</Card>
</div>

<ConfirmDialog
  open={confirming}
  onclose={() => (confirming = false)}
  onconfirm={() => {
    confirming = false;
    onstop();
  }}
  title={$t("workflows.stop_confirm_title")}
  message={$t("workflows.stop_confirm_message")}
  confirmLabel={$t("workflows.stop")}
  data-testid="workflow-run-stop-dialog"
/>
