<script lang="ts">
  /**
   * Operator persona: review of the session plan awaiting approval.
   *
   * Renders each step as a structured row (number, title, description,
   * rationale, dependency and tool chips) and offers three paths: approve,
   * request adjustments (a soft-gate reject carrying a reason that drives a
   * revision turn), or keep typing in the conversation. Exhaustive step
   * provenance lives in the Builder variant. Decisions go through
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
    type SessionPlanStep,
  } from "$lib/stores/chatPlanMode";
  import { phaseLabelKey } from "$lib/chat/planPhase";

  interface Props {
    sessionId: string;
  }

  let { sessionId }: Props = $props();

  const planState = $derived($chatPlanState);
  const steps = $derived(planState.plan?.steps ?? []);
  const revision = $derived(planState.plan?.revision ?? 0);

  // step_id -> 1-based position, so dependencies read as "#2" instead of an
  // opaque id.
  const stepNumber = $derived(
    new Map(steps.map((s, i) => [s.step_id, i + 1])),
  );
  function depLabel(deps: string[]): string {
    return deps.map((d) => `#${stepNumber.get(d) ?? "?"}`).join(", ");
  }
  function isPending(step: SessionPlanStep): boolean {
    return !step.status || step.status === "pending";
  }

  let isActing = $state(false);
  let adjusting = $state(false);
  let adjustReason = $state("");

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
      await rejectPlan(sessionId, adjustReason.trim() || undefined);
      setChatPlanRejected();
      adjusting = false;
      adjustReason = "";
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
  <div class="mb-3 flex items-start justify-between gap-2">
    <div class="min-w-0">
      <h2 class="text-sm font-semibold">{$t("chat.planMode.title")}</h2>
      {#if steps.length > 0}
        <p class="mt-0.5 text-[11px] text-muted-foreground" data-testid="chat-plan-review-meta">
          {$t("chat.planMode.stepCount", { values: { count: steps.length } })}
          <span class="text-muted-foreground/50">·</span>
          {$t("chat.planMode.revisionLabel", { values: { n: revision } })}
        </p>
      {/if}
    </div>
    <span
      class="inline-flex shrink-0 items-center gap-1 rounded-full border border-primary/30 bg-primary/10 px-2 py-0.5 text-[10px] font-medium text-primary"
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
    <ol class="mb-4 max-h-[32vh] space-y-1.5 overflow-y-auto pr-1 text-sm">
      {#each steps as step, i (step.step_id)}
        <li
          class="flex items-start gap-2 rounded-md border border-border/50 bg-muted/20 px-2.5 py-1.5"
          data-testid="chat-plan-review-step"
        >
          <span
            class="mt-px flex h-4 w-4 shrink-0 items-center justify-center rounded-full text-[10px] font-semibold tabular-nums {isPending(
              step,
            )
              ? 'bg-primary/10 text-primary'
              : 'bg-primary text-primary-foreground'}"
          >
            {i + 1}
          </span>
          <div class="min-w-0 flex-1">
            <p class="truncate text-[13px] font-medium leading-snug text-foreground">
              {step.title || step.description}
            </p>
            {#if step.title && step.description && step.description !== step.title}
              <p class="line-clamp-1 text-xs leading-snug text-muted-foreground">
                {step.description}
              </p>
            {/if}
            {#if step.depends_on.length > 0}
              <span class="text-[10px] text-muted-foreground/70">
                {$t("chat.planMode.dependenciesLabel")}: {depLabel(step.depends_on)}
              </span>
            {/if}
          </div>
        </li>
      {/each}
    </ol>
  {/if}

  {#if planState.errorMessage}
    <p class="mb-2 text-xs text-destructive" data-testid="chat-plan-review-error">
      {planState.errorMessage}
    </p>
  {/if}

  {#if adjusting}
    <div class="mb-3 space-y-2" data-testid="chat-plan-review-adjust">
      <textarea
        bind:value={adjustReason}
        rows="3"
        placeholder={$t("chat.planMode.requestChangesPlaceholder")}
        class="w-full resize-none rounded-md border border-border bg-background px-2.5 py-2 text-xs text-foreground placeholder:text-muted-foreground/60 focus:outline-none focus:ring-1 focus:ring-primary/40"
      ></textarea>
      <div class="flex gap-2">
        <Button
          variant="default"
          size="sm"
          loading={isActing}
          disabled={isActing}
          onclick={submitAdjust}
          data-testid="chat-plan-review-adjust-send"
        >
          {$t("chat.planMode.send")}
        </Button>
        <Button
          variant="ghost"
          size="sm"
          disabled={isActing}
          onclick={cancelAdjust}
          data-testid="chat-plan-review-adjust-cancel"
        >
          {$t("chat.planMode.cancel")}
        </Button>
      </div>
    </div>
  {:else}
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
        onclick={openAdjust}
        data-testid="chat-plan-review-request-changes"
      >
        {$t("chat.planMode.requestChanges")}
      </Button>
    </div>

    <p class="mt-3 text-xs text-muted-foreground" data-testid="chat-plan-review-hint">
      {$t("chat.planMode.reviseHint")}
    </p>
  {/if}
</section>
