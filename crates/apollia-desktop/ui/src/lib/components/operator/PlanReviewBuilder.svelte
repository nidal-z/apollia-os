<script lang="ts">
  // Builder mode: exhaustive plan review with per-step detail and replan diff.
  // Approval actions reuse the same IPC as the Operator view (PlanReview).
  // The edit flow lives in PlanEditFlow.
  import { t } from "svelte-i18n";
  import { Button } from "$lib/components/ui/button";
  import {
    planModeState,
    setPlanApproved,
    setPlanRejected,
    setPlanEditing,
    setPlanError,
  } from "$lib/stores/planMode";
  import { submitPlanDecision } from "$lib/ipc/planMode";
  import PlanStepDetail from "./PlanStepDetail.svelte";
  import { computeDiff, resolveDependencyLabels } from "./planDiff";

  interface Props {
    /** Run identifier of the gate to resolve. */
    runId: string;
    /** Enables the edit entry point (wired by the edit flow). */
    canEdit?: boolean;
  }

  let { runId, canEdit = false }: Props = $props();

  const plan = $derived($planModeState.plan);
  const previousPlan = $derived($planModeState.previousPlan);
  const status = $derived($planModeState.status);
  const isPlanValid = $derived(plan !== null && plan.steps.length > 0);
  const stepsWithDiff = $derived(computeDiff(plan, previousPlan));
  const isReplan = $derived(previousPlan !== null);

  let isActing = $state(false);

  async function act(decision: "approve" | "reject"): Promise<void> {
    if (isActing) return;
    isActing = true;
    try {
      await submitPlanDecision(runId, { decision });
      if (decision === "approve") setPlanApproved();
      else setPlanRejected();
    } catch (err: unknown) {
      setPlanError(err instanceof Error ? err.message : String(err));
    } finally {
      isActing = false;
    }
  }
</script>

<section
  class="rounded-lg border border-border bg-card p-4 text-card-foreground"
  data-testid="plan-review-builder"
>
  <h2 class="mb-3 text-sm font-semibold">
    {$t("plan_mode.proposed_plan_title")}
  </h2>

  {#if !isPlanValid}
    <p class="text-sm text-destructive" data-testid="plan-review-error">
      {$t("plan_mode.error_malformed_plan")}
    </p>
  {:else if status === "approved"}
    <p class="text-sm text-success">{$t("plan_mode.status_approved")}</p>
  {:else}
    {#if isReplan}
      <p class="mb-2 text-xs text-warning" data-testid="plan-replan-notice">
        {$t("plan_mode.replan_notice")}
      </p>
    {/if}

    <ol class="mb-4 space-y-2">
      {#each stepsWithDiff as entry (entry.step.step_id + entry.diffStatus)}
        <PlanStepDetail
          step={entry.step}
          diffStatus={entry.diffStatus}
          resolvedDependencies={resolveDependencyLabels(
            entry.step.depends_on,
            plan?.steps ?? [],
          )}
        />
      {/each}
    </ol>

    {#if $planModeState.errorMessage}
      <p class="mb-2 text-xs text-destructive" data-testid="plan-review-ipc-error">
        {$planModeState.errorMessage}
      </p>
    {/if}

    <div class="flex gap-2">
      <Button
        variant="default"
        size="sm"
        loading={isActing}
        disabled={isActing}
        onclick={() => act("approve")}
        data-testid="plan-review-approve"
      >
        {$t("plan_mode.approve")}
      </Button>
      <Button
        variant="outline"
        size="sm"
        disabled={isActing}
        onclick={() => act("reject")}
        data-testid="plan-review-reject"
      >
        {$t("plan_mode.reject")}
      </Button>
      {#if canEdit}
        <Button
          variant="ghost"
          size="sm"
          disabled={isActing}
          onclick={setPlanEditing}
          data-testid="plan-review-edit"
        >
          {$t("plan_mode.edit_plan")}
        </Button>
      {/if}
    </div>
  {/if}
</section>
