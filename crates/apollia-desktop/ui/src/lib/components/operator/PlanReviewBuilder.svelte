<script lang="ts">
  // Builder mode: exhaustive review of the plan awaiting approval.
  //
  // Same focal card and always-visible approval bar as the Operator view, plus
  // per-step provenance detail (step id, tool / model hints, resolved
  // dependencies, replan diff), a replan notice and the raw plan JSON behind a
  // disclosure. Approval reuses the run-keyed IPC (`$lib/ipc/planMode`); the
  // edit flow is entered through the bar. No direct invoke here.
  import { t } from "svelte-i18n";
  import { Disclosure } from "$lib/components/ui/disclosure";
  import { TriangleAlert } from "lucide-svelte";
  import {
    planModeState,
    setPlanApproved,
    setPlanRejected,
    setPlanEditing,
    setPlanError,
  } from "$lib/stores/planMode";
  import { submitPlanDecision } from "$lib/ipc/planMode";
  import { computeDiff, type StepDiffStatus } from "./planDiff";
  import { listNavigation } from "./listNavigation";
  import PlanCard from "./PlanCard.svelte";
  import PlanStepRow from "./PlanStepRow.svelte";
  import PlanApprovalBar from "./PlanApprovalBar.svelte";
  import PlanLaunchError from "./PlanLaunchError.svelte";

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
  const steps = $derived(plan?.steps ?? []);
  const isPlanValid = $derived(plan !== null && steps.length > 0);
  const stepsWithDiff = $derived(computeDiff(plan, previousPlan));
  const isReplan = $derived(previousPlan !== null);
  const rawJson = $derived(plan ? JSON.stringify(plan, null, 2) : "");

  const stepNumber = $derived(new Map(steps.map((s, i) => [s.step_id, i + 1])));
  function depLabel(deps: string[]): string {
    if (deps.length === 0) return "";
    return deps.map((d) => `#${stepNumber.get(d) ?? d}`).join(", ");
  }

  function diffBadge(
    diff: StepDiffStatus,
  ): { key: string; cls: string } | null {
    if (diff === "added") return { key: "plan_mode.diff_added", cls: "bg-success/10 text-success" };
    if (diff === "removed") return { key: "plan_mode.diff_removed", cls: "bg-destructive/10 text-destructive" };
    if (diff === "modified") return { key: "plan_mode.diff_modified", cls: "bg-warning/10 text-warning" };
    return null;
  }

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

  function onCardKeydown(event: KeyboardEvent): void {
    if ((event.metaKey || event.ctrlKey) && event.key === "Enter") {
      event.preventDefault();
      void act("approve");
    }
  }
</script>

{#if !isPlanValid}
  <PlanCard
    tone="destructive"
    title={$t("plan_mode.error_malformed_plan")}
    testid="plan-review-builder"
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
    testid="plan-review-builder"
  >
    <p class="text-body-sm text-success">{$t("plan_mode.status_approved")}</p>
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
      testid="plan-review-builder"
    >
      {#if isReplan}
        <div
          class="mb-3 flex items-start gap-2 rounded-md border border-warning/30 bg-warning/10 px-3 py-2 text-body-xs text-foreground"
          data-testid="plan-replan-notice"
        >
          <TriangleAlert size={15} class="mt-px flex-none text-warning" aria-hidden="true" />
          <span>{$t("plan_mode.replan_notice")}</span>
        </div>
      {/if}

      <ol
        class="tb-steps max-h-[42vh] list-none overflow-y-auto pr-1"
        use:listNavigation={{ rowSelector: '[data-testid="plan-review-step"]' }}
      >
        {#each stepsWithDiff as entry, i (entry.step.step_id + entry.diffStatus)}
          {@const badge = diffBadge(entry.diffStatus)}
          <PlanStepRow
            index={i + 1}
            title={entry.step.description}
            status={entry.diffStatus === "removed" ? "skipped" : "pending"}
            tomb={entry.diffStatus === "removed"}
            deps={depLabel(entry.step.depends_on)}
            testid="plan-review-step"
          >
            {#snippet detail()}
              <div class="mt-1.5 flex flex-wrap items-center gap-1.5">
                <span
                  class="rounded-full leading-none bg-surface-2 px-2 py-0.5 font-mono text-overline text-muted-foreground"
                >
                  {entry.step.step_id}
                </span>
                {#if entry.step.tool_hint}
                  <span class="rounded-full leading-none bg-accent/10 px-2 py-0.5 text-overline text-accent-foreground">
                    {entry.step.tool_hint}
                  </span>
                {/if}
                {#if entry.step.model_hint}
                  <span class="rounded-full leading-none bg-surface-2 px-2 py-0.5 text-overline text-muted-foreground">
                    {entry.step.model_hint}
                  </span>
                {/if}
                {#if badge}
                  <span
                    class="rounded-full px-2 py-0.5 text-overline uppercase {badge.cls}"
                    data-testid="plan-step-badge"
                  >
                    {$t(badge.key)}
                  </span>
                {/if}
              </div>
            {/snippet}
          </PlanStepRow>
        {/each}
      </ol>

      <div class="mt-3">
        <Disclosure testid="plan-review-raw">
          {#snippet summary()}
            <span class="text-caption font-semibold text-muted-foreground">
              {$t("plan_mode.raw_plan")}
            </span>
          {/snippet}
          {#snippet children()}
            <pre
              class="overflow-x-auto rounded-md bg-surface-2 px-3 py-2 font-mono text-code-sm text-foreground/80">{rawJson}</pre>
          {/snippet}
        </Disclosure>
      </div>

      {#if status === "error" && $planModeState.errorMessage}
        <PlanLaunchError
          raw={$planModeState.errorMessage}
          {isActing}
          onRetry={() => act("approve")}
        />
      {:else}
        <PlanApprovalBar
          approveKey="plan_mode.approve"
          secondaryKey="plan_mode.reject"
          secondaryKbd="R"
          reviseHintKey="plan_mode.revise_hint"
          onEdit={canEdit ? setPlanEditing : null}
          {isActing}
          onApprove={() => act("approve")}
          onSecondary={() => act("reject")}
          approveTestid="plan-review-approve"
          secondaryTestid="plan-review-reject"
        />
      {/if}
    </PlanCard>
  </div>
{/if}
