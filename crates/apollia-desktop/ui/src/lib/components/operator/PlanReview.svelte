<script lang="ts">
  // Operator mode: focal review of the plan awaiting approval.
  //
  // Reads the run-keyed gate from `planModeState` and resolves it through
  // `$lib/ipc/planMode` (no direct invoke). Steps read as plain, numbered rows
  // (chat `.tb-steps` idiom); the approval bar stays anchored below the list in
  // every pending state. "Request changes" is a soft gate: it opens a revision
  // reason, it does not abandon. Keyboard: Cmd+Enter approves, R opens the
  // revision, Escape cancels it. Exhaustive per-step detail lives in the
  // Builder variant (PlanReviewBuilder).
  import { t } from "svelte-i18n";
  import { Button } from "$lib/components/ui/button";
  import {
    planModeState,
    setPlanApproved,
    setPlanRejected,
    setPlanError,
  } from "$lib/stores/planMode";
  import { submitPlanDecision } from "$lib/ipc/planMode";
  import { listNavigation } from "./listNavigation";
  import PlanCard from "./PlanCard.svelte";
  import PlanStepRow from "./PlanStepRow.svelte";
  import PlanApprovalBar from "./PlanApprovalBar.svelte";
  import PlanLaunchError from "./PlanLaunchError.svelte";

  interface Props {
    /** Run identifier of the gate to resolve. */
    runId: string;
  }

  let { runId }: Props = $props();

  const plan = $derived($planModeState.plan);
  const status = $derived($planModeState.status);
  const steps = $derived(plan?.steps ?? []);
  const isPlanValid = $derived(plan !== null && steps.length > 0);

  // step_id -> 1-based position, so dependencies read as "#2", not an opaque id.
  const stepNumber = $derived(new Map(steps.map((s, i) => [s.step_id, i + 1])));
  function depLabel(deps: string[]): string {
    if (deps.length === 0) return "";
    return deps.map((d) => `#${stepNumber.get(d) ?? "?"}`).join(", ");
  }

  let isActing = $state(false);
  let adjusting = $state(false);
  let adjustReason = $state("");

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

  function openAdjust(): void {
    adjusting = true;
  }

  function cancelAdjust(): void {
    adjusting = false;
    adjustReason = "";
  }

  async function submitAdjust(): Promise<void> {
    if (isActing) return;
    isActing = true;
    try {
      await submitPlanDecision(runId, {
        decision: "reject",
        reason: adjustReason.trim() || undefined,
      });
      setPlanRejected();
      adjusting = false;
      adjustReason = "";
    } catch (err: unknown) {
      setPlanError(err instanceof Error ? err.message : String(err));
    } finally {
      isActing = false;
    }
  }

  function onCardKeydown(event: KeyboardEvent): void {
    if ((event.metaKey || event.ctrlKey) && event.key === "Enter") {
      event.preventDefault();
      if (adjusting) void submitAdjust();
      else void handleApprove();
      return;
    }
    const typing = event.target instanceof HTMLTextAreaElement;
    if (!adjusting && !typing && (event.key === "r" || event.key === "R")) {
      event.preventDefault();
      openAdjust();
    } else if (adjusting && event.key === "Escape") {
      event.preventDefault();
      cancelAdjust();
    }
  }
</script>

{#if !isPlanValid}
  <PlanCard
    tone="destructive"
    title={$t("plan_mode.error_malformed_plan")}
    testid="plan-review"
  >
    <p class="text-body-sm text-muted-foreground" data-testid="plan-review-error">
      {$t("plan_mode.error_malformed_plan")}
    </p>
  </PlanCard>
{:else if status === "approved"}
  <PlanCard
    tone="success"
    title={$t("plan_mode.status_approved")}
    phaseKey="chat.planMode.phase.executing"
    phaseTone="exec"
    testid="plan-review"
  >
    <p class="text-body-sm text-success">{$t("plan_mode.status_approved")}</p>
  </PlanCard>
{:else if status === "rejected"}
  <PlanCard title={$t("plan_mode.proposed_plan_title")} testid="plan-review">
    <p class="text-body-sm text-muted-foreground">
      {$t("plan_mode.status_rejected")}
    </p>
  </PlanCard>
{:else}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div onkeydown={onCardKeydown}>
    <PlanCard
      focal
      title={$t("plan_mode.proposed_plan_title")}
      stepCount={steps.length}
      revision={null}
      phaseKey="plan_mode.awaiting_approval"
      phaseTone="awaiting"
      testid="plan-review"
    >
      <ol
        class="tb-steps max-h-[32vh] list-none overflow-y-auto pr-1"
        use:listNavigation={{ rowSelector: '[data-testid="plan-review-step"]' }}
      >
        {#each steps as step, i (step.step_id)}
          <PlanStepRow
            index={i + 1}
            title={step.description}
            deps={depLabel(step.depends_on)}
            testid="plan-review-step"
          />
        {/each}
      </ol>

      {#if status === "error" && $planModeState.errorMessage}
        <PlanLaunchError
          raw={$planModeState.errorMessage}
          {isActing}
          onRetry={handleApprove}
        />
      {:else if adjusting}
        <div
          class="mt-3 space-y-2 border-t border-border/60 pt-3"
          data-testid="plan-review-adjust"
        >
          <label
            class="block text-caption font-semibold text-muted-foreground"
            for="plan-adjust-reason"
          >
            {$t("plan_mode.request_changes_label")}
          </label>
          <textarea
            id="plan-adjust-reason"
            bind:value={adjustReason}
            rows="3"
            placeholder={$t("plan_mode.request_changes_placeholder")}
            class="w-full resize-none rounded-md border border-border bg-background px-2.5 py-2 text-body-sm text-foreground placeholder:text-muted-foreground/60 focus:border-primary/50 focus:outline-none focus:ring-2 focus:ring-primary/15"
          ></textarea>
          <div class="flex gap-2">
            <Button
              variant="primary-gradient"
              size="sm"
              loading={isActing}
              disabled={isActing}
              onclick={submitAdjust}
              data-testid="plan-review-adjust-send"
            >
              {$t("plan_mode.send")}
            </Button>
            <Button
              variant="ghost"
              size="sm"
              disabled={isActing}
              onclick={cancelAdjust}
              data-testid="plan-review-adjust-cancel"
            >
              {$t("plan_mode.cancel")}
            </Button>
          </div>
        </div>
      {:else}
        <PlanApprovalBar
          approveKey="plan_mode.approve_and_launch"
          secondaryKey="plan_mode.request_changes"
          secondaryKbd="R"
          reviseHintKey="plan_mode.revise_hint"
          {isActing}
          onApprove={handleApprove}
          onSecondary={openAdjust}
          approveTestid="plan-review-approve"
          secondaryTestid="plan-review-request-changes"
        />
      {/if}
    </PlanCard>
  </div>
{/if}
