<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import { slide } from "svelte/transition";
  import { HelpCircle } from "lucide-svelte";
  import { Spinner } from "$lib/components/ui/progress";
  import { Button } from "$lib/components/ui/button";
  import AskUserQuestion from "./AskUserQuestion.svelte";
  import AskUserSummary from "./AskUserSummary.svelte";

  interface UserQuestion {
    id: string;
    question: string;
    type: "open" | "single_choice" | "multi_choice";
    options?: string[];
    hint?: string;
  }

  interface UserAnswer {
    id: string;
    value?: string | null;
    values?: string[];
    skipped: boolean;
  }

  interface Props {
    requestId: string;
    sessionId: string;
    questions: UserQuestion[];
    context?: string | null;
  }

  let { requestId, sessionId, questions, context = null }: Props = $props();

  // Per-question state
  let openValues = $state<Record<string, string>>({});
  let selectedValues = $state<Record<string, string[]>>({});
  let isProcessing = $state(false);
  let isSubmitted = $state(false);
  let submittedAnswers = $state<UserAnswer[]>([]);
  let error = $state<string | null>(null);

  const canSubmit = $derived(() => {
    return questions.some((q) => {
      if (q.type === "open") return (openValues[q.id] ?? "").trim().length > 0;
      if (q.type === "single_choice") return (openValues[q.id] ?? "").length > 0;
      if (q.type === "multi_choice") return (selectedValues[q.id] ?? []).length > 0;
      return false;
    });
  });

  function buildAnswers(skipped: boolean): UserAnswer[] {
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

  async function handleSubmit(): Promise<void> {
    isProcessing = true;
    error = null;
    const answers = buildAnswers(false);
    try {
      await invoke("respond_user_input", { requestId, answers });
      submittedAnswers = answers;
      isSubmitted = true;
    } catch (err: unknown) {
      error = err instanceof Error ? err.message : String(err);
      isProcessing = false;
    }
  }

  async function handleSkip(): Promise<void> {
    isProcessing = true;
    error = null;
    const answers = buildAnswers(true);
    try {
      await invoke("respond_user_input", { requestId, answers });
      submittedAnswers = answers;
      isSubmitted = true;
    } catch (err: unknown) {
      error = err instanceof Error ? err.message : String(err);
      isProcessing = false;
    }
  }
</script>

{#if isSubmitted}
  <AskUserSummary {questions} answers={submittedAnswers} />
{:else}
  <div
    class="my-1.5 glass-card glass-border rounded-lg border-l-2 border-l-info px-3 py-2.5 text-xs"
    data-testid="ask-user-card-{requestId}"
    transition:slide={{ duration: 200 }}
  >
    <!-- Header -->
    <div class="flex items-center gap-2 font-medium text-foreground">
      <div class="flex h-6 w-6 items-center justify-center rounded-lg bg-info/10">
        <HelpCircle class="h-3.5 w-3.5 text-info" />
      </div>
      <span class="text-sm">{$t("chat.ask_user_title")}</span>
    </div>

    <!-- Context (optional) -->
    {#if context}
      <p class="mt-1 text-[11px] italic text-muted-foreground">{context}</p>
    {/if}

    <!-- Questions -->
    <div class="max-h-[60vh] overflow-y-auto">
      {#each questions as question, i}
        <AskUserQuestion
          {question}
          index={i}
          disabled={isProcessing}
          value={openValues[question.id] ?? ""}
          selectedValues={selectedValues[question.id] ?? []}
          onvaluechange={(v) => (openValues[question.id] = v)}
          onselectedchange={(v) => (selectedValues[question.id] = v)}
        />
      {/each}
    </div>

    <!-- Error -->
    {#if error}
      <p class="mt-1.5 text-[10px] text-destructive">{error}</p>
    {/if}

    <!-- Actions -->
    <div class="mt-3 flex justify-end gap-2">
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
      <Button
        size="sm"
        class="text-[11px] h-7 px-4"
        disabled={isProcessing || !canSubmit()}
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
