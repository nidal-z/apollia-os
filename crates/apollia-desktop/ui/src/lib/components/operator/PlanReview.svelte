<script lang="ts">
  // Operator mode: compact review of the plan awaiting approval.
  // Step-level detail, dependency resolution and replan diff live in the
  // Builder view (PlanReviewBuilder); the edit flow lives in PlanEditFlow.
  import { t } from "svelte-i18n";
  import { Button } from "$lib/components/ui/button";
  import {
    planModeState,
    setPlanApproved,
    setPlanRejected,
    setPlanError,
  } from "$lib/stores/planMode";
  import { submitPlanDecision } from "$lib/ipc/planMode";

  interface Props {
    /** Run identifier of the gate to resolve. */
    runId: string;
  }

  let { runId }: Props = $props();

  const plan = $derived($planModeState.plan);
  const status = $derived($planModeState.status);
  const isPlanValid = $derived(plan !== null && plan.steps.length > 0);

  let isActing = $state(false);

  async function handleApprove(): Promise<void> {
    if (isActing) return;
    isActing = true;
    try {
      await submitPlanDecision(runId, { decision: "approve" });
      setPlanApproved();
    } catch (err: unknown) {
      setPlanError(err instanceof Error ? err.message : String(err));
    } finally {
      isActing = false;
    }
  }

  async function handleReject(): Promise<void> {
    if (isActing) return;
    isActing = true;
    try {
      await submitPlanDecision(runId, { decision: "reject" });
      setPlanRejected();
    } catch (err: unknown) {
      setPlanError(err instanceof Error ? err.message : String(err));
    } finally {
      isActing = false;
    }
  }
</script>

<section
  class="rounded-lg border border-border bg-card p-4 text-card-foreground"
  data-testid="plan-review"
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
  {:else if status === "rejected"}
    <p class="text-sm text-muted-foreground">
      {$t("plan_mode.status_rejected")}
    </p>
  {:else}
    <p class="mb-1 text-xs font-medium text-muted-foreground">
      {$t("plan_mode.steps_label")}
    </p>
    <ol class="mb-4 list-decimal space-y-1 pl-5 text-sm">
      {#each plan?.steps ?? [] as step (step.step_id)}
        <li>{step.description}</li>
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
        onclick={handleApprove}
        data-testid="plan-review-approve"
      >
        {$t("plan_mode.approve")}
      </Button>
      <Button
        variant="outline"
        size="sm"
        disabled={isActing}
        onclick={handleReject}
        data-testid="plan-review-reject"
      >
        {$t("plan_mode.reject")}
      </Button>
    </div>
  {/if}
</section>
