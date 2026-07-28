<script lang="ts">
  // One plan step rendered with its full detail and replan diff badge.
  // Pure presentation: receives already-resolved dependency labels.
  import { t } from "svelte-i18n";
  import type { PlanStep } from "$lib/ipc/planMode";
  import type { StepDiffStatus } from "./planDiff";

  interface Props {
    step: PlanStep;
    diffStatus?: StepDiffStatus;
    resolvedDependencies?: string[];
  }

  let {
    step,
    diffStatus = "unchanged",
    resolvedDependencies = [],
  }: Props = $props();

  const badge = $derived(
    diffStatus === "added"
      ? { key: "plan_mode.diff_added", cls: "text-success" }
      : diffStatus === "removed"
        ? { key: "plan_mode.diff_removed", cls: "text-destructive" }
        : diffStatus === "modified"
          ? { key: "plan_mode.diff_modified", cls: "text-warning" }
          : null,
  );

  const isRemoved = $derived(diffStatus === "removed");
</script>

<li
  class="rounded-md border border-border bg-surface-1 p-3"
  data-testid="plan-step-detail"
  data-diff={diffStatus}
>
  <div class="mb-1 flex items-center gap-2">
    <span class="font-mono text-xs text-muted-foreground">{step.step_id}</span>
    {#if badge}
      <span
        class={`text-overline uppercase ${badge.cls}`}
        data-testid="plan-step-badge"
      >
        {$t(badge.key)}
      </span>
    {/if}
  </div>

  <p class={`text-sm ${isRemoved ? "text-muted-foreground line-through" : ""}`}>
    {step.description}
  </p>

  {#if resolvedDependencies.length > 0}
    <p class="mt-1 text-xs text-muted-foreground">
      {$t("plan_mode.step_dependencies_label")}: {resolvedDependencies.join(", ")}
    </p>
  {/if}
  {#if step.tool_hint}
    <p class="mt-1 text-xs text-muted-foreground">
      {$t("plan_mode.step_tool_label")}: <span class="font-mono">{step.tool_hint}</span>
    </p>
  {/if}
  {#if step.model_hint}
    <p class="mt-1 text-xs text-muted-foreground">
      {$t("plan_mode.step_model_label")}: <span class="font-mono">{step.model_hint}</span>
    </p>
  {/if}
</li>
