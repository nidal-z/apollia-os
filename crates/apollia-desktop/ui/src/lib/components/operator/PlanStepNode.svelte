<script lang="ts">
  // Custom xyflow node for a single plan step.
  //
  // Structure and live layout come from `PlanDagPanel`; this component renders
  // the card. The fine-grained status colors, chips, rationale and tombstone
  // styling land in the follow-up node-rendering story.
  import { Handle, Position, type NodeProps } from "@xyflow/svelte";
  import { t } from "svelte-i18n";
  import type { StepNodeData } from "./planDagLayout";

  let { data }: NodeProps = $props();
  const node = $derived(data as StepNodeData);
  const step = $derived(node.step);
</script>

<Handle type="target" position={Position.Top} />
<div
  class="w-[220px] rounded-lg border border-border bg-surface-1 px-3 py-2"
  data-testid="plan-step-node"
  data-status={step.status}
>
  <div class="mb-1 flex items-center justify-between gap-2">
    <span class="text-[10px] font-medium text-muted-foreground">{step.status}</span>
    <span class="font-mono text-[9px] text-muted-foreground">{step.step_id}</span>
  </div>
  <p class="truncate text-[12px] font-medium text-foreground">
    {step.title || $t("plan_session.untitled_step")}
  </p>
  {#if step.description}
    <p class="mt-0.5 line-clamp-2 text-[11px] text-muted-foreground">{step.description}</p>
  {/if}
</div>
<Handle type="source" position={Position.Bottom} />
