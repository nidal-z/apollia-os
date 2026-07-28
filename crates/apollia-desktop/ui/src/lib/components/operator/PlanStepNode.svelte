<script lang="ts">
  // Custom xyflow node for a single plan step.
  //
  // The card color derives exclusively from `stepStatusToken(status)`; the
  // current step (in_progress) carries a highlight ring and a subtle pulse.
  // Density follows the UI mode via `nodeFields($uiMode)`: Operator stays
  // compact (title, status and the provenance chip), Builder exposes the
  // description, dependencies, tool/model hints, rationale and change reason.
  // A step dropped by a replan renders as a faded tombstone. The card is
  // keyboard-activable (Enter/Space) to open the trace drawer.
  import { Handle, Position, type NodeProps } from "@xyflow/svelte";
  import { t } from "svelte-i18n";
  import { toStepStatus } from "$lib/ipc/plan";
  import { uiMode } from "$lib/stores/mode";
  import { stepStatusToken } from "./stepStatusToken";
  import { originToChip, hasReason } from "./provenanceChip";
  import { nodeFields } from "./planNodeDensity";
  import { PLAN_SESSION_KEYS } from "$lib/i18n/strings/planSession";
  import PlanThinkingOverlay from "./PlanThinkingOverlay.svelte";
  import PlanDecisionPopover from "./PlanDecisionPopover.svelte";
  import type { StepNodeData } from "./planDagLayout";

  let { data }: NodeProps = $props();
  const node = $derived(data as StepNodeData);
  const step = $derived(node.step);
  const index = $derived(node.index ?? 0);
  const removed = $derived(node.removed === true);
  const status = $derived(toStepStatus(step.status));
  const tokens = $derived(stepStatusToken(status));
  const isCurrent = $derived(status === "in_progress");

  // Single source of density truth: the node never tests `$uiMode` inline.
  const fields = $derived(nodeFields($uiMode));

  // Provenance chip is always rendered (even an initial step shows its origin).
  // The reason section is gated on a non-blank `provenance.reason` AND the
  // Builder density, so a compact node or a step without a reason shows only
  // the chip, never "undefined".
  const provChip = $derived(originToChip(step.provenance.origin));
  const reason = $derived(step.provenance.reason);
  const showReason = $derived(fields.showReason && hasReason(reason));

  // Thinking trace and decision point for this step's turn. Both are optional:
  // a turn without a trace or a decision renders nothing extra. The decision
  // popover self-gates to Builder; the thinking overlay stays light in Operator.
  const thinking = $derived(node.thinking ?? null);
  const decisionPoint = $derived(node.decisionPoint ?? null);

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
  <div class="mb-1 flex items-center gap-2">
    {#if index > 0}
      <span
        class="flex h-4 w-4 shrink-0 items-center justify-center rounded-full bg-primary/15 text-overline font-semibold tabular-nums text-primary"
        data-testid="plan-step-number"
      >
        {index}
      </span>
    {/if}
    <span class="text-overline font-medium {tokens.text}">{$t(tokens.labelKey)}</span>
    <span
      class="rounded-full px-1.5 py-0.5 text-overline font-medium {provChip.tokenClass}"
      data-testid="plan-step-provenance"
      data-origin={typeof step.provenance.origin === "string"
        ? step.provenance.origin
        : "replan"}
    >
      {$t(provChip.labelKey, { values: provChip.labelValues })}
    </span>
    <span class="ml-auto font-mono text-overline text-muted-foreground">{step.step_id}</span>
  </div>
  <p
    class="truncate text-body-xs font-medium text-foreground {removed
      ? 'line-through'
      : ''}"
  >
    {step.title || step.description || $t("plan_session.untitled_step")}
  </p>
  {#if fields.showDescription && step.description && step.title}
    <p class="mt-0.5 line-clamp-2 text-caption text-muted-foreground">{step.description}</p>
  {/if}
  {#if fields.showDependencies && step.depends_on.length > 0}
    <p class="mt-1 text-caption text-muted-foreground" data-testid="plan-step-dependencies">
      <span class="font-medium">{$t(PLAN_SESSION_KEYS.dependenciesLabel)}:</span>
      {step.depends_on.join(", ")}
    </p>
  {/if}
  {#if fields.showHints && (step.tool_hint || step.model_hint)}
    <div class="mt-1.5 flex flex-wrap gap-1">
      {#if step.tool_hint}
        <span class="rounded bg-accent/10 px-1.5 py-0.5 text-overline text-accent-foreground"
          >{step.tool_hint}</span
        >
      {/if}
      {#if step.model_hint}
        <span class="rounded bg-muted/20 px-1.5 py-0.5 text-overline text-muted-foreground"
          >{step.model_hint}</span
        >
      {/if}
    </div>
  {/if}
  {#if fields.showRationale && step.rationale}
    <button
      type="button"
      class="mt-1 text-overline text-muted-foreground hover:text-foreground"
      onclick={(event) => {
        event.stopPropagation();
        expanded = !expanded;
      }}
      aria-expanded={expanded}
    >
      {$t(expanded ? "plan_session.hide_rationale" : "plan_session.show_rationale")}
    </button>
    {#if expanded}
      <p class="mt-1 text-caption text-muted-foreground">{step.rationale}</p>
    {/if}
  {/if}
  {#if showReason}
    <p class="mt-1 text-caption text-muted-foreground" data-testid="plan-step-reason">
      <span class="font-medium">{$t(PLAN_SESSION_KEYS.reasonLabel)}:</span>
      {reason}
    </p>
  {/if}
  {#if thinking}
    <PlanThinkingOverlay content={thinking.content} live={thinking.live} />
  {/if}
  {#if decisionPoint}
    <PlanDecisionPopover point={decisionPoint} />
  {/if}
</div>
<Handle type="source" position={Position.Bottom} />
