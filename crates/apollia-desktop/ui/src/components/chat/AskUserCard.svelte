<script lang="ts">
  /**
   * AskUserCard v2 (US-SP42-032)
   * ============================
   * Compact, a11y-first ask-user surface:
   *   - header = prompt (1 line, click-to-expand when overflowing)
   *   - body   = questions list scrollable to 480 px max
   *   - footer = actions (Skip / Skip with default / Submit) + waiting timer
   *   - role="alertdialog" + Esc cancels the card, Enter submits when allowed
   *
   * Post-submit the card collapses to `AskUserSummary` — behaviour preserved
   * from v1 (cf. docs/internal/UX-UI-AUDIT-CHAT-COMPONENTS-ORPHELINS.md §3.2).
   */

  import { onMount, tick } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import { slide } from "svelte/transition";
  import { HelpCircle } from "lucide-svelte";
  import { Spinner } from "$lib/components/ui/progress";
  import { Button } from "$lib/components/ui/button";
  import AskUserQuestion from "./AskUserQuestion.svelte";
  import AskUserSummary from "./AskUserSummary.svelte";
  import ApprovalTimer from "./ApprovalTimer.svelte";

  interface UserQuestion {
    id: string;
    question: string;
    type: "open" | "single_choice" | "multi_choice";
    options?: string[];
    hint?: string;
    /** Optional default answer surfaced by the "Skip with default" action. */
    default?: string | string[];
  }

  interface UserAnswer {
    id: string;
    value?: string | null;
    values?: string[];
    skipped: boolean;
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
  let isSubmitted = $state(false);
  let submittedAnswers = $state<UserAnswer[]>([]);
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
  function buildAnswers(skipped: boolean, useDefaults = false): UserAnswer[] {
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

  async function sendAnswers(answers: UserAnswer[]): Promise<void> {
    isProcessing = true;
    error = null;
    try {
      await invoke("respond_user_input", { requestId, answers });
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

  // ── A11y — initial focus + keyboard handlers ─────────────────────────────
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
  <div
    bind:this={rootEl}
    role="alertdialog"
    aria-modal="false"
    aria-labelledby="ask-user-title-{requestId}"
    tabindex="-1"
    onkeydown={handleKeydown}
    class="my-1.5 glass-card glass-border rounded-lg border-l-2 border-l-info px-3 py-2 text-xs"
    data-testid="ask-user-card-{requestId}"
    transition:slide={{ duration: 200 }}
  >
    <!-- Header — compact (prompt 1 line, expandable) -->
    <div class="flex items-start gap-2">
      <div class="mt-0.5 flex h-5 w-5 flex-shrink-0 items-center justify-center rounded-md bg-info/10">
        <HelpCircle class="h-3 w-3 text-info" />
      </div>

      <button
        id="ask-user-title-{requestId}"
        type="button"
        class="flex-1 text-left"
        aria-expanded={promptExpanded}
        aria-controls="ask-user-prompt-{requestId}"
        onclick={() => (promptExpanded = !promptExpanded)}
        data-testid="ask-user-prompt-toggle"
      >
        <span class="text-[12px] font-medium text-foreground">
          {$t("chat.ask_user_title")}
        </span>
        <span
          id="ask-user-prompt-{requestId}"
          class="ml-1 text-[12px] text-muted-foreground {promptExpanded ? '' : 'line-clamp-1'}"
        >
          — {firstQuestionText}
        </span>
      </button>
    </div>

    <!-- Waiting timer -->
    <div class="mt-1">
      <ApprovalTimer startedAt={startedAtMs} totalMs={null} />
    </div>

    <!-- Context (optional, rendered when provided) -->
    {#if context}
      <p class="mt-1 text-[11px] italic text-muted-foreground">{context}</p>
    {/if}

    <!-- Questions — scroll max 480 px -->
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
            class="mt-1 text-right text-[10px] {cur > charLimit ? 'text-destructive' : 'text-muted-foreground'}"
            data-testid="ask-user-charcount-{question.id}"
          >
            {cur} / {charLimit}
          </p>
        {/if}
      {/each}
    </div>

    <!-- Error -->
    {#if error}
      <p class="mt-1.5 text-[10px] text-destructive" role="alert">{error}</p>
    {/if}

    <!-- Actions -->
    <div class="mt-2 flex flex-wrap items-center justify-end gap-2">
      <Button
        variant="ghost"
        size="sm"
        class="text-[11px] h-7 px-3"
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
          class="text-[11px] h-7 px-3"
          disabled={isProcessing}
          onclick={handleSkipWithDefault}
          data-testid="ask-user-skip-default"
        >
          {$t("chat.ask_user_skip_default")}
        </Button>
      {/if}
      <Button
        size="sm"
        class="text-[11px] h-7 px-4"
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
{/if}
