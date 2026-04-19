<script lang="ts">
  import { t } from "svelte-i18n";
  import { slide } from "svelte/transition";
  import { Check, ChevronRight, ChevronDown } from "lucide-svelte";
  import { Separator } from "$lib/components/ui/separator";

  interface UserAnswer {
    id: string;
    value?: string | null;
    values?: string[];
    skipped: boolean;
  }

  interface UserQuestion {
    id: string;
    question: string;
    type: "open" | "single_choice" | "multi_choice";
    options?: string[];
    hint?: string;
  }

  interface Props {
    questions: UserQuestion[];
    answers: UserAnswer[];
  }

  let { questions, answers }: Props = $props();

  let expanded = $state(false);

  const answeredCount = $derived(answers.filter((a) => !a.skipped).length);

  function answerText(answer: UserAnswer): string {
    if (answer.skipped) return $t("chat.ask_user_skipped_label");
    if (answer.values && answer.values.length > 0) return answer.values.join(", ");
    return answer.value ?? "";
  }

  function questionLabel(answerId: string): string {
    return questions.find((q) => q.id === answerId)?.question ?? answerId;
  }
</script>

<div
  class="my-1.5 glass-surface glass-border-subtle rounded-lg text-xs"
  data-testid="ask-user-summary"
  transition:slide={{ duration: 200 }}
>
  <button
    class="flex w-full items-center gap-2 px-3 py-2 text-left"
    onclick={() => (expanded = !expanded)}
    data-testid="ask-user-summary-toggle"
  >
    <Check class="h-3.5 w-3.5 text-success" />
    <span class="flex-1 text-[11px] font-medium text-foreground">
      {$t("chat.ask_user_answered", { values: { count: answeredCount } })}
    </span>
    {#if expanded}
      <ChevronDown class="h-3.5 w-3.5 text-muted-foreground" />
    {:else}
      <ChevronRight class="h-3.5 w-3.5 text-muted-foreground" />
    {/if}
  </button>

  {#if expanded}
    <div class="px-3 py-2 space-y-2" transition:slide={{ duration: 150 }}>
      <Separator />
      {#each answers as answer}
        <div>
          <p class="text-[10px] font-medium text-muted-foreground">{questionLabel(answer.id)}</p>
          <p class="text-[11px] text-foreground {answer.skipped ? 'italic text-muted-foreground' : ''}">
            {answerText(answer)}
          </p>
        </div>
      {/each}
    </div>
  {/if}
</div>
