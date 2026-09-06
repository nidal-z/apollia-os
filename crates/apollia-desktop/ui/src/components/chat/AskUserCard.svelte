<script lang="ts">
  /**
   * AskUserCard v2
   * ============================
   * Compact, a11y-first ask-user surface:
   *   - header = prompt (1 line, click-to-expand when overflowing)
   *   - body   = questions list scrollable to 480 px max
   *   - footer = actions (Skip / Submit) + waiting timer
   *   - role="alertdialog" + Esc cancels the card, Enter submits when allowed
   *
   * The card has no post-submit state of its own: the runtime emits
   * `ChatUserInputResolved` while it delivers the answers, and the host
   * (`ChatMessageScroller`) unmounts the card on that event. The answered
   * exchange is then read from the reasoning trace (`ReasoningCard`).
   */

  import { onMount, tick } from "svelte";
  import { t } from "svelte-i18n";
  import { slide } from "svelte/transition";
  import { HelpCircle } from "lucide-svelte";
  import { Spinner } from "$lib/components/ui/progress";
  import { respondUserInput } from "$lib/ipc/inbox";
  import type { AskUserAnswer } from "$lib/types";
  import { Button } from "$lib/components/ui/button";
  import AskUserQuestion from "./AskUserQuestion.svelte";
  import ApprovalTimer from "$lib/components/operator/approval/ApprovalTimer.svelte";

  interface UserQuestion {
    id: string;
    question: string;
    type: "open" | "single_choice" | "multi_choice";
    options?: string[];
    hint?: string;
  }

  interface Props {
    requestId: string;
    questions: UserQuestion[];
    context?: string | null;
    /** Epoch-ms when the runtime asked the question (for the urgency timer). */
    startedAtMs?: number;
    /** Per-question character budget for open questions. 0 = unlimited. */
    charLimit?: number;
  }

  let {
    requestId,
    questions,
    context = null,
    startedAtMs = Date.now(),
    charLimit = 2_000,
  }: Props = $props();

  // ── Per-question state ───────────────────────────────────────────────────
  let openValues = $state<Record<string, string>>({});
  let selectedValues = $state<Record<string, string[]>>({});
  let isProcessing = $state(false);
  let error = $state<string | null>(null);
  let promptExpanded = $state(false);
  let rootEl: HTMLDivElement | undefined = $state();

  // ── Derived ──────────────────────────────────────────────────────────────
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
  function buildAnswers(skipped: boolean): AskUserAnswer[] {
    return questions.map((q) => {
      if (skipped) {
        return { id: q.id, value: null, values: [], skipped: true };
      }
      if (q.type === "multi_choice") {
        const vals = selectedValues[q.id] ?? [];
        return { id: q.id, value: null, values: vals, skipped: vals.length === 0 };
      }
      const val = (openValues[q.id] ?? "").trim();
      return { id: q.id, value: val || null, values: [], skipped: val.length === 0 };
    });
  }

  /**
   * Deliver the answers. On success the card stays in its processing state:
   * the runtime has already emitted `ChatUserInputResolved`, which unmounts
   * this card from the scroller, so there is nothing left to render here.
   */
  async function sendAnswers(answers: AskUserAnswer[]): Promise<void> {
    isProcessing = true;
    error = null;
    try {
      await respondUserInput(requestId, answers);
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

  // ── A11y - initial focus + keyboard handlers ─────────────────────────────
  onMount(async () => {
    await tick();
    const firstInput = rootEl?.querySelector<HTMLElement>(
      "input, textarea, [role='radio']",
    );
    firstInput?.focus();
  });

  function handleKeydown(e: KeyboardEvent): void {
    if (isProcessing) return;
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

  <!-- Error -->
  {#if error}
    <p class="mt-1.5 text-micro text-destructive" role="alert">{error}</p>
  {/if}

  <!-- Actions -->
  <div class="mt-2 flex flex-wrap items-center justify-end gap-2">
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
  </div>
</div>
