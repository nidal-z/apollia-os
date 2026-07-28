<script lang="ts">
  // Per-step trace drawer.
  //
  // Reuses `PlanStepDetail` for the step fields and adds the execution trace
  // (step id, task id, tool calls and their outputs). Built on the canonical
  // `Sheet` drawer primitive, which traps Escape to close and manages the
  // backdrop. Read-only: it never mutates the plan store.
  import { t } from "svelte-i18n";
  import { Sheet, SheetHeader, SheetContent } from "$lib/components/ui/sheet";
  import PlanStepDetail from "./PlanStepDetail.svelte";
  import type { SessionPlanStep } from "$lib/stores/chatPlanMode";

  /** A single tool call recorded during the step execution. */
  export interface ToolCallTrace {
    name: string;
    output: string;
  }

  interface Props {
    open: boolean;
    step: SessionPlanStep | null;
    taskId?: string | null;
    toolCalls?: ToolCallTrace[];
    onclose: () => void;
  }

  let {
    open,
    step,
    taskId = null,
    toolCalls = [],
    onclose,
  }: Props = $props();
</script>

<Sheet {open} {onclose} side="right" width="sm" ariaLabel={step?.title ?? ""}>
  {#if step}
    <SheetHeader
      title={step.title}
      onclose={onclose}
      closeLabel={$t("a11y.close")}
    />
    <SheetContent padding="compact" data-testid="plan-step-drawer">
      <div class="font-mono text-overline text-muted-foreground">
        {$t("plan_session.trace_step_id")}: {step.step_id}
        {#if taskId}
          · {$t("plan_session.trace_task_id")}: {taskId}
        {/if}
      </div>

      <ul class="list-none p-0">
        <PlanStepDetail {step} resolvedDependencies={[]} />
      </ul>

      <div>
        <h4 class="mb-1 text-caption font-medium text-foreground">
          {$t("plan_session.trace_tool_calls")}
        </h4>
        {#if toolCalls.length === 0}
          <p class="text-caption text-muted-foreground">{$t("plan_session.trace_empty")}</p>
        {:else}
          <ol class="space-y-2">
            {#each toolCalls as call, i (i)}
              <li class="rounded-md border border-border bg-surface-1 p-2">
                <div class="font-mono text-overline text-accent-foreground">{call.name}</div>
                <pre
                  class="mt-1 whitespace-pre-wrap text-overline text-muted-foreground">{call.output}</pre>
              </li>
            {/each}
          </ol>
        {/if}
      </div>
    </SheetContent>
  {/if}
</Sheet>
