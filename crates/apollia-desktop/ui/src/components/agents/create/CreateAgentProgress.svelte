<script lang="ts">
  /**
   * CreateAgentProgress - the agent workshop at work: its three steps, each
   * done, in progress or still to come, and the question it waits on, asked
   * here with its proposed answers. The person picks one, writes their own,
   * or passes; the workshop goes on either way.
   */
  import { t } from "svelte-i18n";
  import { CheckCircle, Circle, Loader2, XCircle } from "lucide-svelte";
  import { Card } from "$lib/components/operator";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { STEP_LABEL, type StepView, type WorkshopQuestion } from "./agentDraft";

  interface Props {
    steps: StepView[];
    /** The question the workshop waits on, when it waits on one. */
    question: WorkshopQuestion | null;
    /** An answer is being sent. */
    answering: boolean;
    /** Send `answer`; `null` passes the question. */
    onanswer: (answer: string | null) => void;
  }

  let { steps, question, answering, onanswer }: Props = $props();

  let own = $state("");

  // A new question starts with an empty answer of one's own. The question is
  // read again every second and handed over as a new object each time: only
  // its identifier says whether it is another question, so what is being
  // typed survives the reads.
  const asked = $derived(question?.id ?? null);
  $effect(() => {
    void asked;
    own = "";
  });

  function sendOwn(): void {
    const text = own.trim();
    if (text) onanswer(text);
  }
</script>

<div class="max-w-3xl space-y-4" data-testid="agent-create-progress">
  <Card class="p-3.5">
    <ol class="m-0 list-none space-y-2.5 p-0">
      {#each steps as view (view.step)}
        <li
          class="flex items-center gap-2.5"
          data-testid="agent-create-step"
          data-step={view.step}
          data-state={view.state}
        >
          {#if view.state === "done"}
            <CheckCircle size={14} class="text-success-a11y" aria-hidden="true" />
          {:else if view.state === "current"}
            <Loader2 size={14} class="animate-spin text-muted-foreground" aria-hidden="true" />
          {:else if view.state === "failed"}
            <XCircle size={14} class="text-destructive" aria-hidden="true" />
          {:else}
            <Circle size={14} class="text-muted-foreground/50" aria-hidden="true" />
          {/if}
          <span
            class="text-body-xs {view.state === 'upcoming' ? 'text-muted-foreground/70' : 'text-foreground'}"
          >
            {$t(STEP_LABEL[view.step])}
          </span>
        </li>
      {/each}
    </ol>
  </Card>
  {#if question}
    <Card class="space-y-3 p-3.5" data-testid="agent-create-question">
      <p class="m-0 text-body-sm font-medium text-foreground" data-testid="agent-create-question-text">
        {question.question}
      </p>
      {#if question.options.length > 0}
        <div class="flex flex-wrap gap-2">
          {#each question.options as option (option)}
            <Button
              variant="outline"
              size="sm"
              onclick={() => onanswer(option)}
              disabled={answering}
              data-testid="agent-create-question-option"
            >
              {option}
            </Button>
          {/each}
        </div>
      {/if}
      <div class="flex items-center gap-2">
        <Input
          bind:value={own}
          placeholder={$t("agent_create.question_own_placeholder")}
          aria-label={$t("agent_create.question_own_label")}
          disabled={answering}
          onkeydown={(event: KeyboardEvent) => {
            if (event.key === "Enter") sendOwn();
          }}
          data-testid="agent-create-question-own"
        />
        <Button
          variant="primary-solid"
          size="sm"
          onclick={sendOwn}
          disabled={answering || own.trim().length === 0}
          data-testid="agent-create-question-send"
        >
          {$t("agent_create.question_send")}
        </Button>
        <Button
          variant="ghost"
          size="sm"
          onclick={() => onanswer(null)}
          disabled={answering}
          data-testid="agent-create-question-skip"
        >
          {$t("agent_create.question_skip")}
        </Button>
      </div>
    </Card>
  {/if}
</div>
