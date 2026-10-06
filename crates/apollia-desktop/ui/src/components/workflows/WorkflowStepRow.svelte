<script lang="ts">
  /**
   * WorkflowStepRow - one step of a workflow. Read from the workflow, it is
   * numbered and named. Read from a run, it also says where it stands (done,
   * in progress, to come, failed, stopped, not launched), how long it took,
   * and what it returned or why it failed.
   */
  import { t } from "svelte-i18n";
  import { Check, Circle, CircleSlash, OctagonX, X } from "lucide-svelte";
  import { Badge } from "$lib/components/ui/badge";
  import { Spinner } from "$lib/components/ui/progress";
  import { formatElapsedClock } from "$lib/utils";
  import type { WorkflowStep } from "$lib/ipc/workflows";
  import { elapsedMs, stepLabelKey, stepTone } from "./workflowRuns";

  interface Props {
    /** Its position among the steps, from 1. */
    position: number;
    name: string;
    /** The step of a run, absent when the workflow is only read. */
    step?: WorkflowStep;
    now: number;
  }

  let { position, name, step, now }: Props = $props();

  const status = $derived(step?.status ?? "declared");
  const muted = $derived(status === "pending" || status === "not_started");
  const elapsed = $derived(step ? elapsedMs(step.startedAt, step.endedAt, now) : null);
</script>

<li
  class="flex items-start gap-3 rounded-md border border-border/60 px-3 py-2.5 {muted ? 'opacity-60' : ''}"
  data-testid="workflow-step"
  data-step={name}
  data-status={status}
>
  <span
    class="inline-flex h-6 w-6 shrink-0 items-center justify-center rounded-full border border-border bg-surface-1 text-caption font-semibold text-muted-foreground"
    aria-hidden="true"
  >
    {#if status === "completed"}
      <Check size={12} class="text-success-a11y" />
    {:else if status === "running"}
      <Spinner size={12} />
    {:else if status === "failed"}
      <X size={12} class="text-destructive" />
    {:else if status === "canceled"}
      <OctagonX size={12} class="text-warning-a11y" />
    {:else if status === "not_started"}
      <CircleSlash size={12} />
    {:else if status === "pending"}
      <Circle size={10} />
    {:else}
      {position}
    {/if}
  </span>
  <div class="min-w-0 flex-1">
    <div class="flex flex-wrap items-center gap-2">
      <span class="text-body-xs font-medium text-foreground">{name}</span>
      {#if step}
        <Badge size="sm" variant={stepTone(step.status)} data-testid="workflow-step-status">
          {$t(stepLabelKey(step.status))}
        </Badge>
        {#if elapsed !== null && step.status !== "pending" && step.status !== "not_started"}
          <span class="text-caption text-muted-foreground">{formatElapsedClock(Math.floor(elapsed / 1000))}</span>
        {/if}
      {/if}
    </div>
    {#if step?.error}
      <p class="m-0 mt-1 whitespace-pre-wrap break-words text-caption text-destructive" data-testid="workflow-step-error">
        {step.error}
      </p>
    {:else if step?.output}
      <p class="m-0 mt-1 line-clamp-3 whitespace-pre-wrap break-words text-caption text-muted-foreground" data-testid="workflow-step-output">
        {step.output}
      </p>
    {/if}
  </div>
</li>
