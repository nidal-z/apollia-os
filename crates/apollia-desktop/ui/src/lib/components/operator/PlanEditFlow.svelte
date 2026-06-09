<script lang="ts">
  // Plan edit flow: locally edit steps, then submit as an edited plan that the
  // engine executes directly (no replan round trip). Validation is local and
  // synchronous (fail fast); the global store is updated only on success.
  import { t } from "svelte-i18n";
  import { Button } from "$lib/components/ui/button";
  import {
    planModeState,
    setPlanApproved,
    setPlanError,
  } from "$lib/stores/planMode";
  import { submitPlanDecision, type PlanStep } from "$lib/ipc/planMode";
  import PlanStepEditor from "./PlanStepEditor.svelte";
  import { validate } from "./planEditValidation";

  interface Props {
    /** Run identifier of the gate to resolve. */
    runId: string;
  }

  let { runId }: Props = $props();

  // Local mutable copy, seeded from the store's pending plan.
  let editableSteps = $state<PlanStep[]>(
    ($planModeState.plan?.steps ?? []).map((s) => ({ ...s })),
  );

  const validationErrors = $derived(validate(editableSteps));
  const hasErrors = $derived(validationErrors.length > 0);
  const cyclicError = $derived(
    validationErrors.find((e) => e.kind === "cyclic_dependency") ?? null,
  );

  let isSubmitting = $state(false);

  function errorForStep(stepId: string): string | null {
    return (
      validationErrors.find(
        (e) => e.kind === "empty_label" && e.stepId === stepId,
      )?.message ?? null
    );
  }

  function updateStepLabel(stepId: string, label: string): void {
    editableSteps = editableSteps.map((s) =>
      s.step_id === stepId ? { ...s, description: label } : s,
    );
  }

  function removeStep(stepId: string): void {
    editableSteps = editableSteps.filter((s) => s.step_id !== stepId);
  }

  async function handleSubmit(): Promise<void> {
    if (hasErrors || isSubmitting) return;
    isSubmitting = true;
    try {
      await submitPlanDecision(runId, {
        decision: "edit",
        revised_steps: editableSteps,
      });
      setPlanApproved();
    } catch (err: unknown) {
      setPlanError(err instanceof Error ? err.message : String(err));
    } finally {
      isSubmitting = false;
    }
  }

  function handleCancel(): void {
    // Discard local edits and return to the review state.
    planModeState.update((s) => ({ ...s, status: "pending_approval" }));
  }
</script>

<section
  class="rounded-lg border border-border bg-card p-4 text-card-foreground"
  data-testid="plan-edit-flow"
>
  <h2 class="mb-3 text-sm font-semibold">
    {$t("plan_mode.proposed_plan_title")}
  </h2>

  <ul class="mb-3 space-y-2">
    {#each editableSteps as step (step.step_id)}
      <PlanStepEditor
        {step}
        errorMessage={errorForStep(step.step_id)}
        onLabelChange={updateStepLabel}
        onRemove={removeStep}
      />
    {/each}
  </ul>

  {#if cyclicError}
    <p
      class="mb-3 rounded-md bg-destructive/10 px-3 py-2 text-xs text-destructive"
      data-testid="plan-edit-cycle-error"
    >
      {$t(cyclicError.message)}
    </p>
  {/if}

  {#if $planModeState.errorMessage}
    <p class="mb-2 text-xs text-destructive" data-testid="plan-edit-ipc-error">
      {$planModeState.errorMessage}
    </p>
  {/if}

  <div class="flex gap-2">
    <Button
      variant="default"
      size="sm"
      loading={isSubmitting}
      disabled={hasErrors || isSubmitting}
      onclick={handleSubmit}
      data-testid="plan-edit-submit"
    >
      {$t("plan_mode.submit_revised_plan")}
    </Button>
    <Button
      variant="outline"
      size="sm"
      disabled={isSubmitting}
      onclick={handleCancel}
      data-testid="plan-edit-cancel"
    >
      {$t("plan_mode.cancel_edit")}
    </Button>
  </div>
</section>
