<script lang="ts">
  /**
   * Operator persona: compact review of the session plan awaiting approval.
   *
   * Shows the current phase, the plan steps, Approve / Reject actions, and a
   * hint that the user can keep talking to revise. Step-level detail (deps,
   * rationale, provenance) lives in the Builder variant. Decisions go through
   * `$lib/ipc/planMode`; no `invoke` from this component.
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

  interface Props {
    sessionId: string;
  }

  let { sessionId }: Props = $props();

  const planState = $derived($chatPlanState);
  const steps = $derived(planState.plan?.steps ?? []);

  let isActing = $state(false);

  async function handleApprove(): Promise<void> {
    if (isActing) return;
    isActing = true;
    try {
      await approvePlan(sessionId);
      setChatPlanApproved();
    } catch (err: unknown) {
      setChatPlanError(err instanceof Error ? err.message : String(err));
    } finally {
      isActing = false;
    }
  }

  async function handleReject(): Promise<void> {
    if (isActing) return;
    isActing = true;
    try {
      await rejectPlan(sessionId);
      setChatPlanRejected();
    } catch (err: unknown) {
      setChatPlanError(err instanceof Error ? err.message : String(err));
    } finally {
      isActing = false;
    }
  }
</script>

<section
  class="rounded-lg border border-border bg-card p-4 text-card-foreground shadow-elev-1"
  data-testid="chat-plan-review"
>
  <div class="mb-3 flex items-center justify-between gap-2">
    <h2 class="text-sm font-semibold">{$t("chat.planMode.title")}</h2>
    <span
      class="inline-flex items-center gap-1 rounded-full border border-primary/30 bg-primary/10 px-2 py-0.5 text-[10px] font-medium text-primary"
      data-testid="chat-plan-review-phase"
    >
      {$t(phaseLabelKey(planState.phase))}
    </span>
  </div>

  {#if steps.length === 0}
    <p class="text-sm text-muted-foreground" data-testid="chat-plan-review-empty-steps">
      {$t("chat.planMode.empty")}
    </p>
  {:else}
    <p class="mb-1 text-xs font-medium text-muted-foreground">
      {$t("chat.planMode.stepsLabel")}
    </p>
    <ol class="mb-4 list-decimal space-y-1 pl-5 text-sm">
      {#each steps as step (step.step_id)}
        <li>{step.title || step.description}</li>
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
      onclick={handleApprove}
      data-testid="chat-plan-review-approve"
    >
      {$t("chat.planMode.approve")}
    </Button>
    <Button
      variant="outline"
      size="sm"
      disabled={isActing}
      onclick={handleReject}
      data-testid="chat-plan-review-reject"
    >
      {$t("chat.planMode.reject")}
    </Button>
  </div>

  <p class="mt-3 text-xs text-muted-foreground" data-testid="chat-plan-review-hint">
    {$t("chat.planMode.reviseHint")}
  </p>
</section>
