<script lang="ts">
  /**
   * Builder persona: exhaustive review of the session plan awaiting approval.
   *
   * Adds per-step detail (dependencies, rationale, provenance) and the plan
   * revision on top of the Approve / Reject actions and the revise hint.
   * Decisions go through `$lib/ipc/planMode`; no `invoke` from this component.
   */
  import { t } from "svelte-i18n";
  import { Button } from "$lib/components/ui/button";
  import { approvePlan, rejectPlan } from "$lib/ipc/planMode";
  import {
    chatPlanState,
    setChatPlanApproved,
    setChatPlanRejected,
    setChatPlanError,
  } from "$lib/stores/chatPlanMode";
  import { phaseLabelKey } from "$lib/chat/planPhase";
  import { originToChip } from "$lib/components/operator/provenanceChip";

  interface Props {
    sessionId: string;
  }

  let { sessionId }: Props = $props();

  const planState = $derived($chatPlanState);
  const plan = $derived(planState.plan);
  const steps = $derived(plan?.steps ?? []);

  let isActing = $state(false);

  async function act(kind: "approve" | "reject"): Promise<void> {
    if (isActing) return;
    isActing = true;
    try {
      if (kind === "approve") {
        await approvePlan(sessionId);
        setChatPlanApproved();
      } else {
        await rejectPlan(sessionId);
        setChatPlanRejected();
      }
    } catch (err: unknown) {
      setChatPlanError(err instanceof Error ? err.message : String(err));
    } finally {
      isActing = false;
    }
  }
</script>

<section
  class="rounded-lg border border-border bg-card p-4 text-card-foreground shadow-elev-1"
  data-testid="chat-plan-review-builder"
>
  <div class="mb-3 flex items-center justify-between gap-2">
    <h2 class="text-sm font-semibold">{$t("chat.planMode.title")}</h2>
    <div class="flex items-center gap-2">
      {#if plan}
        <span class="text-micro text-muted-foreground" data-testid="chat-plan-review-revision">
          r{plan.revision}
        </span>
      {/if}
      <span
        class="inline-flex items-center gap-1 rounded-full border border-primary/30 bg-primary/10 px-2 py-0.5 text-micro font-medium text-primary"
        data-testid="chat-plan-review-phase"
      >
        {$t("chat.planMode.phaseLabel")}: {$t(phaseLabelKey(planState.phase))}
      </span>
    </div>
  </div>

  {#if steps.length === 0}
    <p class="text-sm text-muted-foreground" data-testid="chat-plan-review-empty-steps">
      {$t("chat.planMode.empty")}
    </p>
  {:else}
    <ol class="mb-4 max-h-80 space-y-2 overflow-y-auto overscroll-contain pr-1">
      {#each steps as step (step.step_id)}
        {@const provenanceChip = originToChip(step.provenance.origin)}
        <li class="rounded-md border border-border/60 bg-muted/30 p-2.5 text-sm">
          <p class="font-medium">{step.title || step.description}</p>
          {#if step.title && step.description && step.description !== step.title}
            <p class="mt-0.5 text-xs text-muted-foreground">{step.description}</p>
          {/if}

          {#if step.depends_on.length > 0}
            <p class="mt-1 text-caption text-muted-foreground">
              <span class="font-medium">{$t("chat.planMode.dependenciesLabel")}:</span>
              {step.depends_on.join(", ")}
            </p>
          {/if}

          <div class="mt-1 flex flex-wrap gap-x-3 gap-y-0.5 text-caption text-muted-foreground">
            {#if step.tool_hint}
              <span><span class="font-medium">{$t("chat.planMode.toolLabel")}:</span> {step.tool_hint}</span>
            {/if}
            {#if step.model_hint}
              <span><span class="font-medium">{$t("chat.planMode.modelLabel")}:</span> {step.model_hint}</span>
            {/if}
            <span
              ><span class="font-medium">{$t("chat.planMode.provenanceLabel")}:</span>
              {$t(provenanceChip.labelKey, { values: provenanceChip.labelValues })}</span
            >
          </div>

          {#if step.rationale}
            <p class="mt-1 text-caption text-muted-foreground">
              <span class="font-medium">{$t("chat.planMode.rationaleLabel")}:</span>
              {step.rationale}
            </p>
          {/if}
        </li>
      {/each}
    </ol>
  {/if}

  {#if planState.errorMessage}
    <p class="mb-2 text-xs text-destructive" data-testid="chat-plan-review-error">
      {planState.errorMessage}
    </p>
  {/if}

  <div class="flex gap-2">
    <Button
      variant="default"
      size="sm"
      loading={isActing}
      disabled={isActing}
      onclick={() => act("approve")}
      data-testid="chat-plan-review-approve"
    >
      {$t("chat.planMode.approve")}
    </Button>
    <Button
      variant="outline"
      size="sm"
      disabled={isActing}
      onclick={() => act("reject")}
      data-testid="chat-plan-review-reject"
    >
      {$t("chat.planMode.reject")}
    </Button>
  </div>

  <p class="mt-3 text-xs text-muted-foreground" data-testid="chat-plan-review-hint">
    {$t("chat.planMode.reviseHint")}
  </p>
</section>
