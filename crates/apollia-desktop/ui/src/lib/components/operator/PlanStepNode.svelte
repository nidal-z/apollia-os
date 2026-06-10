<script lang="ts">
  // Custom xyflow node for a single plan step.
  //
  // The card color derives exclusively from `stepStatusToken(status)`; the
  // current step (in_progress) carries a highlight ring and a subtle pulse.
  // Tool/model hints render as chips when present, the rationale reveals on
  // expand, and a step dropped by a replan renders as a faded tombstone.
  // The card is keyboard-activable (Enter/Space) to open the trace drawer.
  import { Handle, Position, type NodeProps } from "@xyflow/svelte";
  import { t } from "svelte-i18n";
  import { toStepStatus } from "$lib/ipc/plan";
  import { stepStatusToken } from "./stepStatusToken";
  import type { StepNodeData } from "./planDagLayout";

  let { data }: NodeProps = $props();
  const node = $derived(data as StepNodeData);
  const step = $derived(node.step);
  const removed = $derived(node.removed === true);
  const status = $derived(toStepStatus(step.status));
  const tokens = $derived(stepStatusToken(status));
  const isCurrent = $derived(status === "in_progress");

  let expanded = $state(false);

  function activate(): void {
    node.onSelect?.(step);
  }

  function onKeydown(event: KeyboardEvent): void {
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      activate();
    }
  }
</script>

<Handle type="target" position={Position.Top} />
<div
  role="button"
  tabindex="0"
  class="w-[220px] cursor-pointer rounded-lg border px-3 py-2 transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring {tokens.border} {tokens.surface}
    {isCurrent ? 'ring-2 ring-ring' : ''}
    {tokens.pulse ? 'animate-pulse' : ''}
    {removed ? 'opacity-50' : ''}"
  data-testid="plan-step-node"
  data-status={status}
  data-removed={removed}
  onclick={activate}
  onkeydown={onKeydown}
>
  <div class="mb-1 flex items-center justify-between gap-2">
    <span class="text-[10px] font-medium {tokens.text}">{$t(tokens.labelKey)}</span>
    <span class="font-mono text-[9px] text-muted-foreground">{step.step_id}</span>
  </div>
  <p
    class="truncate text-[12px] font-medium text-foreground {removed
      ? 'line-through'
      : ''}"
  >
    {step.title || $t("plan_session.untitled_step")}
  </p>
  {#if step.description}
    <p class="mt-0.5 line-clamp-2 text-[11px] text-muted-foreground">{step.description}</p>
  {/if}
  {#if step.tool_hint || step.model_hint}
    <div class="mt-1.5 flex flex-wrap gap-1">
      {#if step.tool_hint}
        <span class="rounded bg-accent/10 px-1.5 py-0.5 text-[9px] text-accent-foreground"
          >{step.tool_hint}</span
        >
      {/if}
      {#if step.model_hint}
        <span class="rounded bg-muted/20 px-1.5 py-0.5 text-[9px] text-muted-foreground"
          >{step.model_hint}</span
        >
      {/if}
    </div>
  {/if}
  {#if step.rationale}
    <button
      type="button"
      class="mt-1 text-[10px] text-muted-foreground hover:text-foreground"
      onclick={(event) => {
        event.stopPropagation();
        expanded = !expanded;
      }}
      aria-expanded={expanded}
    >
      {$t(expanded ? "plan_session.hide_rationale" : "plan_session.show_rationale")}
    </button>
    {#if expanded}
      <p class="mt-1 text-[11px] text-muted-foreground">{step.rationale}</p>
    {/if}
  {/if}
</div>
<Handle type="source" position={Position.Bottom} />
