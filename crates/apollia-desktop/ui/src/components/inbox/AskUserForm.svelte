<script lang="ts">
  /**
   * Dynamic form for the `ask_user` tool.
   *
   * Renders one input widget per question (open / single_choice /
   * multi_choice), packs the answers into the `AskUserAnswer[]` shape the
   * backend expects, and falls back to `skipped: true` when a field is left
   * empty (validation soft — no blocking).
   */
  import { t } from "svelte-i18n";
  import { Input } from "$lib/components/ui/input";
  import { RadioGroup, RadioItem } from "$lib/components/ui/radio";
  import { Checkbox } from "$lib/components/ui/checkbox";
  import { Button } from "$lib/components/ui/button";
  import RejectReasonDialog from "./RejectReasonDialog.svelte";
  import { buildAskUserAnswers } from "./askUserForm";
  import type { AskUserAnswer, AskUserQuestion } from "$lib/types";

  interface Props {
    questions: AskUserQuestion[];
    context?: string;
    submitting?: boolean;
    /** Chat session that originated the `ask_user` call — enables the
     *  "Ouvrir la conversation" affordance when set. */
    sessionId?: string;
    onsubmit: (answers: AskUserAnswer[]) => void;
    oncancel: (reason: string) => void;
    /** Called when the operator wants to jump to the source conversation
     *  instead of answering inline. Implementer is expected to navigate
     *  to /chat and dispatch the relevant deep-link event. */
    onOpenChat?: () => void;
  }

  let {
    questions,
    context,
    submitting = false,
    sessionId,
    onsubmit,
    oncancel,
    onOpenChat,
  }: Props = $props();

  // Per-question state, keyed by question.id.
  let openValues = $state<Record<string, string>>({});
  let singleValues = $state<Record<string, string>>({});
  let multiValues = $state<Record<string, Record<string, boolean>>>({});

  let rejectOpen = $state(false);

  function handleSubmit() {
    if (submitting) return;
    const answers: AskUserAnswer[] = buildAskUserAnswers(questions, {
      open: openValues,
      single: singleValues,
      multi: multiValues,
    });
    onsubmit(answers);
  }

  function handleReject(reason: string) {
    rejectOpen = false;
    oncancel(reason);
  }
</script>

<div class="flex flex-col gap-4" data-testid="ask-user-form">
  {#if context}
    <div
      class="rounded-md border border-blue-500/30 bg-blue-500/[0.08] px-3 py-2 text-sm text-foreground"
      data-testid="ask-user-context"
    >
      {context}
    </div>
  {/if}

  <div class="flex flex-col gap-5">
    {#each questions as q (q.id)}
      <div class="flex flex-col gap-2" data-testid="ask-user-question-{q.id}">
        <label class="text-sm font-medium text-foreground" for="ask-user-{q.id}">
          {q.question}
        </label>

        {#if q.type === "open"}
          <Input
            id="ask-user-{q.id}"
            bind:value={openValues[q.id]}
            placeholder={q.hint ?? ""}
            data-testid="ask-user-input-{q.id}"
          />
        {:else if q.type === "single_choice"}
          <RadioGroup bind:value={singleValues[q.id]} data-testid="ask-user-radio-{q.id}">
            {#each q.options as opt}
              <RadioItem
                value={opt}
                checked={singleValues[q.id] === opt}
                onchange={(v) => (singleValues[q.id] = v)}
                data-testid="ask-user-radio-{q.id}-{opt}"
              >
                {opt}
              </RadioItem>
            {/each}
          </RadioGroup>
        {:else if q.type === "multi_choice"}
          <div
            class="grid grid-cols-1 gap-2 sm:grid-cols-2"
            data-testid="ask-user-checkboxes-{q.id}"
          >
            {#each q.options as opt}
              <label class="flex items-center gap-2 text-sm">
                <Checkbox
                  checked={multiValues[q.id]?.[opt] ?? false}
                  onchange={(c) => {
                    multiValues[q.id] = { ...(multiValues[q.id] ?? {}), [opt]: c };
                  }}
                  data-testid="ask-user-check-{q.id}-{opt}"
                />
                <span>{opt}</span>
              </label>
            {/each}
          </div>
        {/if}
      </div>
    {/each}
  </div>

  <div class="flex flex-wrap items-center justify-between gap-2 pt-2">
    {#if sessionId && onOpenChat}
      <Button
        variant="ghost"
        onclick={onOpenChat}
        disabled={submitting}
        data-testid="ask-user-open-chat"
      >
        {$t("inbox.ask_user.open_chat")}
      </Button>
    {:else}
      <span></span>
    {/if}

    <div class="flex items-center gap-2">
      <Button
        variant="ghost"
        class="text-destructive hover:text-destructive"
        onclick={() => (rejectOpen = true)}
        disabled={submitting}
        data-testid="ask-user-refuse"
      >
        {$t("inbox.ask_user.refuse")}
      </Button>
      <Button
        variant="default"
        onclick={handleSubmit}
        disabled={submitting}
        data-testid="ask-user-submit"
      >
        {submitting ? $t("inbox.ask_user.submitting") : $t("inbox.ask_user.submit")}
      </Button>
    </div>
  </div>
</div>

<RejectReasonDialog
  open={rejectOpen}
  title={$t("inbox.ask_user.refuse_title")}
  {submitting}
  onconfirm={handleReject}
  onclose={() => (rejectOpen = false)}
/>
