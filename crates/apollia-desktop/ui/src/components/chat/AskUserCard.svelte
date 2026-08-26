<script lang="ts">
  /**
   * AskUserCard v2
   * ============================
   * Compact, a11y-first ask-user surface:
   *   - header = prompt (1 line, click-to-expand when overflowing)
   *   - body   = questions list scrollable to 480 px max
   *   - footer = actions (Skip / Skip with default / Submit) + waiting timer
   *   - role="alertdialog" + Esc cancels the card, Enter submits when allowed
   *
   * Post-submit the card collapses to `AskUserSummary`.
   */

  import { onMount, tick } from "svelte";
  import { t } from "svelte-i18n";
  import { slide } from "svelte/transition";
  import { HelpCircle } from "lucide-svelte";
  import { Spinner } from "$lib/components/ui/progress";
  import { respondUserInput, respondUserInputRejected } from "$lib/ipc/inbox";
  import type { AskUserAnswer } from "$lib/types";
  import { Button } from "$lib/components/ui/button";
  import { Textarea } from "$lib/components/ui/textarea";
  import AskUserQuestion from "./AskUserQuestion.svelte";
  import AskUserSummary from "./AskUserSummary.svelte";
  import ApprovalTimer from "$lib/components/operator/approval/ApprovalTimer.svelte";
  import RiskBadge, { type ImpactLevel } from "$lib/components/operator/badges/RiskBadge.svelte";
  import { Badge } from "$lib/components/ui/badge";

  interface UserQuestion {
    id: string;
    question: string;
    type: "open" | "single_choice" | "multi_choice";
    options?: string[];
    hint?: string;
    /** Optional default answer surfaced by the "Skip with default" action. */
    default?: string | string[];
  }

  /**
   * Structured HITL context surfaced on the card.
   * When provided, the card renders Why / Risk / If-approved / If-rejected
   * blocks on top of the question list. When omitted, the legacy v2 layout
   * is preserved.
   */
  interface HitlContext {
    /** Agent thinking excerpt shown in the "Why" section. */
    contextThinking?: string | null;
    /** Coarse-grained impact level (drives the risk badge colour). */
    estimatedImpact: ImpactLevel;
    /** Structured risk breakdown (always-on static analyzer). */
    riskAnalysis: {
      categories: string[];
      severity: number;
      mitigations: string[];
    };
    /** Plain-text narration of what happens if the user approves. */
    consequencesIfApproved?: string | null;
    /** Plain-text narration of what happens if the user rejects. */
    consequencesIfRejected?: string | null;
  }

  interface Props {
    requestId: string;
    questions: UserQuestion[];
    context?: string | null;
    /** Epoch-ms when the runtime asked the question (for the urgency timer). */
    startedAtMs?: number;
    /** Per-question character budget for open questions. 0 = unlimited. */
    charLimit?: number;
    /** Optional HITL context for Why / Risk / Consequences sections. */
    hitl?: HitlContext | null;
  }

  let {
    requestId,
    questions,
    context = null,
    startedAtMs = Date.now(),
    charLimit = 2_000,
    hitl = null,
  }: Props = $props();

  const MIN_REJECT_REASON_LENGTH = 10;
  let showRejectForm = $state(false);
  let rejectReason = $state("");
  const rejectReasonValid = $derived(
    rejectReason.trim().length >= MIN_REJECT_REASON_LENGTH,
  );

  // ── Per-question state ───────────────────────────────────────────────────
  let openValues = $state<Record<string, string>>({});
  let selectedValues = $state<Record<string, string[]>>({});
  let isProcessing = $state(false);
  let isSubmitted = $state(false);
  let submittedAnswers = $state<AskUserAnswer[]>([]);
  let error = $state<string | null>(null);
  let promptExpanded = $state(false);
  let rootEl: HTMLDivElement | undefined = $state();

  // ── Derived ──────────────────────────────────────────────────────────────
  const hasDefaults = $derived(questions.some((q) => q.default !== undefined));

  const canSubmit = $derived.by(() => {
    return questions.some((q) => {
      if (q.type === "open") return (openValues[q.id] ?? "").trim().length > 0;
      if (q.type === "single_choice") return (openValues[q.id] ?? "").length > 0;
      if (q.type === "multi_choice") return (selectedValues[q.id] ?? []).length > 0;
      return false;
    });
  });

  const anyOverLimit = $derived.by(() => {
    if (charLimit <= 0) return false;
    return Object.values(openValues).some((v) => v.length > charLimit);
  });

  const firstQuestionText = $derived(
    questions[0]?.question ?? $t("chat.ask_user_title"),
  );

  // ── Helpers ──────────────────────────────────────────────────────────────
  function buildAnswers(skipped: boolean, useDefaults = false): AskUserAnswer[] {
    return questions.map((q) => {
      if (skipped && !useDefaults) {
        return { id: q.id, value: null, values: [], skipped: true };
      }
      if (useDefaults && q.default !== undefined) {
        if (Array.isArray(q.default)) {
          return {
            id: q.id,
            value: null,
            values: q.default,
            skipped: q.default.length === 0,
          };
        }
        return { id: q.id, value: q.default, values: [], skipped: false };
      }
      if (q.type === "multi_choice") {
        const vals = selectedValues[q.id] ?? [];
        return { id: q.id, value: null, values: vals, skipped: vals.length === 0 };
      }
      const val = (openValues[q.id] ?? "").trim();
      return { id: q.id, value: val || null, values: [], skipped: val.length === 0 };
    });
  }

  async function sendAnswers(answers: AskUserAnswer[]): Promise<void> {
    isProcessing = true;
    error = null;
    try {
      await respondUserInput(requestId, answers);
      submittedAnswers = answers;
      isSubmitted = true;
    } catch (err: unknown) {
      error = err instanceof Error ? err.message : String(err);
      isProcessing = false;
    }
  }

  async function handleSubmit(): Promise<void> {
    if (!canSubmit || anyOverLimit) return;
    await sendAnswers(buildAnswers(false));
  }

  async function handleSkip(): Promise<void> {
    await sendAnswers(buildAnswers(true));
  }

  async function handleSkipWithDefault(): Promise<void> {
    await sendAnswers(buildAnswers(false, true));
  }

  /**
   * Reject the HITL request. Forces a non-empty reason (min
   * `MIN_REJECT_REASON_LENGTH` chars) - the textarea is mandatory.
   * The reason is forwarded to the agent via `approval_outcome` (SDK-side).
   */
  async function handleRejectConfirm(): Promise<void> {
    if (!rejectReasonValid) return;
    isProcessing = true;
    error = null;
    try {
      await respondUserInputRejected(requestId, rejectReason.trim());
      submittedAnswers = buildAnswers(true);
      isSubmitted = true;
    } catch (err: unknown) {
      error = err instanceof Error ? err.message : String(err);
      isProcessing = false;
    }
  }

  // ── A11y - initial focus + keyboard handlers ─────────────────────────────
  onMount(async () => {
    await tick();
    const firstInput = rootEl?.querySelector<HTMLElement>(
      "input, textarea, [role='radio']",
    );
    firstInput?.focus();
  });

  function handleKeydown(e: KeyboardEvent): void {
    if (isProcessing || isSubmitted) return;
    if (e.key === "Escape") {
      e.preventDefault();
      void handleSkip();
    }
    if (
      e.key === "Enter" &&
      (e.ctrlKey || e.metaKey) &&
      canSubmit &&
      !anyOverLimit
    ) {
      e.preventDefault();
      void handleSubmit();
    }
  }
</script>

{#if isSubmitted}
  <AskUserSummary {questions} answers={submittedAnswers} {requestId} />
{:else}
  <div class="bg-surface-1 border border-border/60 my-1.5 rounded-lg border-l-2 border-l-info px-3 py-2 text-xs" bind:this={rootEl} role="alertdialog" aria-modal="false" aria-labelledby="ask-user-title-{requestId}" tabindex="-1" onkeydown={handleKeydown} data-testid="ask-user-card-{requestId}" transition:slide={{ duration: 200 }}>
    <!-- Header - compact (prompt 1 line, expandable) -->
    <div class="flex items-start gap-2">
      <div class="mt-0.5 flex h-5 w-5 flex-shrink-0 items-center justify-center rounded-md bg-info/10">
        <HelpCircle class="h-3 w-3 text-info" />
      </div>

      <Button variant="ghost" size="sm"
        id="ask-user-title-{requestId}"
        type="button"
        class="flex-1 text-left"
        aria-expanded={promptExpanded}
        aria-controls="ask-user-prompt-{requestId}"
        onclick={() => (promptExpanded = !promptExpanded)}
        data-testid="ask-user-prompt-toggle"
      >
        <span class="text-body-xs font-medium text-foreground">
          {$t("chat.ask_user_title")}
        </span>
        <span
          id="ask-user-prompt-{requestId}"
          class="ml-1 text-body-xs text-muted-foreground {promptExpanded ? '' : 'line-clamp-1'}"
        >
          - {firstQuestionText}
        </span>
      </Button>
    </div>

    <!-- Waiting timer -->
    <div class="mt-1">
      <ApprovalTimer startedAt={startedAtMs} totalMs={null} />
    </div>

    <!-- Context (optional, rendered when provided) -->
    {#if context}
      <p class="mt-1 text-caption italic text-muted-foreground">{context}</p>
    {/if}

    <!-- HITL structured context -->
    {#if hitl}
      <div class="mt-2 space-y-1.5" data-testid="ask-user-hitl-context">
        <!-- Why + Risk badge row -->
        <div class="flex items-start justify-between gap-2">
          {#if hitl.contextThinking}
            <div class="flex-1">
              <div class="text-micro font-semibold uppercase tracking-wide text-muted-foreground">
                {$t("hitl.section.why")}
              </div>
              <p class="mt-0.5 text-caption text-foreground">
                {hitl.contextThinking}
              </p>
            </div>
          {/if}
          <RiskBadge
            level={hitl.estimatedImpact}
            severity={hitl.riskAnalysis.severity}
          />
        </div>

        <!-- Risk categories + mitigations -->
        {#if hitl.riskAnalysis.categories.length > 0 || hitl.riskAnalysis.mitigations.length > 0}
          <details class="rounded-md bg-muted/40 px-2 py-1" data-testid="ask-user-risk-details">
            <summary class="cursor-pointer text-micro font-semibold uppercase tracking-wide text-muted-foreground">
              {$t("hitl.section.risk")}
            </summary>
            {#if hitl.riskAnalysis.categories.length > 0}
              <div class="mt-1 flex flex-wrap gap-1" data-testid="ask-user-risk-categories">
                {#each hitl.riskAnalysis.categories as cat}
                  <Badge variant="outline" size="sm" class="font-mono">
                    {cat}
                  </Badge>
                {/each}
              </div>
            {/if}
            {#if hitl.riskAnalysis.mitigations.length > 0}
              <ul class="mt-1 list-inside list-disc space-y-0.5 text-caption text-muted-foreground">
                {#each hitl.riskAnalysis.mitigations as mit}
                  <li>{mit}</li>
                {/each}
              </ul>
            {/if}
          </details>
        {/if}

        <!-- If approved / If rejected -->
        <div class="grid gap-1.5 sm:grid-cols-2">
          <div
            class="rounded-md border border-success/30 bg-success/5 px-2 py-1"
            data-testid="ask-user-if-approved"
          >
            <div class="text-micro font-semibold uppercase tracking-wide text-success">
              {$t("hitl.section.if_approved")}
            </div>
            <p class="mt-0.5 text-caption text-foreground">
              {hitl.consequencesIfApproved ?? $t("hitl.risk.fallback_approved")}
            </p>
          </div>
          <div
            class="rounded-md border border-destructive/30 bg-destructive/5 px-2 py-1"
            data-testid="ask-user-if-rejected"
          >
            <div class="text-micro font-semibold uppercase tracking-wide text-destructive">
              {$t("hitl.section.if_rejected")}
            </div>
            <p class="mt-0.5 text-caption text-foreground">
              {hitl.consequencesIfRejected ?? $t("hitl.risk.fallback_rejected")}
            </p>
          </div>
        </div>
      </div>
    {/if}

    <!-- Questions - scroll max 480 px -->
    <div class="mt-1 max-h-[480px] overflow-y-auto pr-1">
      {#each questions as question, i (question.id)}
        <AskUserQuestion
          {question}
          index={i}
          disabled={isProcessing}
          value={openValues[question.id] ?? ""}
          selectedValues={selectedValues[question.id] ?? []}
          onvaluechange={(v) => (openValues[question.id] = v)}
          onselectedchange={(v) => (selectedValues[question.id] = v)}
        />
        {#if question.type === "open" && charLimit > 0}
          {@const cur = (openValues[question.id] ?? "").length}
          <p
            class="mt-1 text-right text-micro {cur > charLimit ? 'text-destructive' : 'text-muted-foreground'}"
            data-testid="ask-user-charcount-{question.id}"
          >
            {cur} / {charLimit}
          </p>
        {/if}
      {/each}
    </div>

    <!-- Reject reason form (HITL only, mandatory textarea) -->
    {#if hitl && showRejectForm}
      <div
        class="mt-2 rounded-md border border-destructive/30 bg-destructive/5 p-2"
        data-testid="ask-user-reject-form"
      >
        <label
          for="ask-user-reject-reason-{requestId}"
          class="mb-1 block text-caption font-medium text-destructive"
        >
          {$t("hitl.reject.reason_required", {
            values: { min: MIN_REJECT_REASON_LENGTH },
          })}
        </label>
        <Textarea
          id="ask-user-reject-reason-{requestId}"
          bind:value={rejectReason}
          rows={2}
          placeholder={$t("hitl.reject.reason_placeholder")}
          data-testid="ask-user-reject-reason"
        />
        <p class="mt-1 text-micro text-muted-foreground">
          {rejectReason.trim().length} / {MIN_REJECT_REASON_LENGTH}
        </p>
      </div>
    {/if}

    <!-- Error -->
    {#if error}
      <p class="mt-1.5 text-micro text-destructive" role="alert">{error}</p>
    {/if}

    <!-- Actions -->
    <div class="mt-2 flex flex-wrap items-center justify-end gap-2">
      {#if hitl && showRejectForm}
        <Button
          variant="ghost"
          size="sm"
          class="text-caption h-7 px-3"
          disabled={isProcessing}
          onclick={() => {
            showRejectForm = false;
            rejectReason = "";
          }}
          data-testid="ask-user-reject-cancel"
        >
          {$t("hitl.reject.cancel")}
        </Button>
        <Button
          size="sm"
          variant="destructive"
          class="text-caption h-7 px-3"
          disabled={isProcessing || !rejectReasonValid}
          onclick={handleRejectConfirm}
          data-testid="ask-user-reject-confirm"
        >
          {#if isProcessing}
            <Spinner class="mr-1.5 h-3 w-3" />
          {/if}
          {$t("hitl.reject.confirm")}
        </Button>
      {:else}
      {#if hitl}
        <Button
          variant="ghost"
          size="sm"
          class="text-caption h-7 px-3 text-destructive hover:bg-destructive/10"
          disabled={isProcessing}
          onclick={() => {
            showRejectForm = true;
            rejectReason = "";
          }}
          data-testid="ask-user-reject-open"
        >
          {$t("hitl.reject.open")}
        </Button>
      {/if}
      <Button
        variant="ghost"
        size="sm"
        class="text-caption h-7 px-3"
        disabled={isProcessing}
        onclick={handleSkip}
        data-testid="ask-user-skip"
      >
        {$t("chat.ask_user_skip")}
      </Button>
      {#if hasDefaults}
        <Button
          variant="outline"
          size="sm"
          class="text-caption h-7 px-3"
          disabled={isProcessing}
          onclick={handleSkipWithDefault}
          data-testid="ask-user-skip-default"
        >
          {$t("chat.ask_user_skip_default")}
        </Button>
      {/if}
      <Button
        size="sm"
        class="text-caption h-7 px-4"
        disabled={isProcessing || !canSubmit || anyOverLimit}
        onclick={handleSubmit}
        data-testid="ask-user-submit"
      >
        {#if isProcessing}
          <Spinner class="mr-1.5 h-3 w-3" />
        {/if}
        {$t("chat.ask_user_submit")}
      </Button>
      {/if}
    </div>
  </div>
{/if}
