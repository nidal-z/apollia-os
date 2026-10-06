<script lang="ts">
  /**
   * The card of an item answered with words: the questions an agent asked,
   * or the typed question of a paused task. The choices the agent proposes
   * are offered as they came, and a free answer beside them whenever the item
   * takes one.
   */
  import { t } from "svelte-i18n";
  import { HelpCircle } from "lucide-svelte";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import { Checkbox } from "$lib/components/ui/checkbox";
  import { Input } from "$lib/components/ui/input";
  import { RadioGroup, RadioItem } from "$lib/components/ui/radio";
  import { Textarea } from "$lib/components/ui/textarea";
  import type { DecideRequest, PendingDecision } from "$lib/ipc/pendingDecisions";
  import {
    canAnswer,
    emptyDraft,
    questionFields,
    questionRequest,
    refusalRequest,
    type QuestionDraft,
    type QuestionField,
  } from "./reviewModel";

  interface Props {
    item: PendingDecision;
    /** The agent that asks, as the screen names it. */
    agentLabel: string;
    disabled?: boolean;
    onanswer: (request: DecideRequest) => void;
  }

  let { item, agentLabel, disabled = false, onanswer }: Props = $props();

  const fields = $derived<QuestionField[]>(questionFields(item));
  /** The sentence above the questions, when it says more than the first of them. */
  const context = $derived(
    item.summary.length > 0 && item.summary !== fields[0]?.text ? item.summary : null,
  );

  let draft = $state<QuestionDraft>(emptyDraft());
  let reasonText = $state("");

  const ready = $derived(canAnswer(fields, draft));
  const takesRefusal = $derived(item.answers.includes("refuse"));

  function pickOne(id: string, value: string): void {
    draft.picked[id] = [value];
  }

  function toggle(id: string, value: string, on: boolean): void {
    const current = draft.picked[id] ?? [];
    draft.picked[id] = on ? [...current, value] : current.filter((v) => v !== value);
  }

  function confirmOptions(): { value: string; label: string }[] {
    return [
      { value: "true", label: $t("review.question.yes") },
      { value: "false", label: $t("review.question.no") },
    ];
  }

  function submit(): void {
    if (!ready || disabled) return;
    onanswer(questionRequest(item, fields, $state.snapshot(draft)));
  }
</script>

<div
  class="my-1.5 rounded-lg border border-border/60 border-l-2 border-l-info bg-surface-1 px-3 py-2.5 text-xs"
  data-testid="review-question"
>
  <div class="flex items-center gap-2 font-medium text-foreground">
    <div class="flex h-6 w-6 items-center justify-center rounded-lg bg-info/10">
      <HelpCircle class="h-3.5 w-3.5 text-info" />
    </div>
    <span class="text-sm">{$t("review.question.heading", { values: { agent: agentLabel } })}</span>
    <Badge variant="info" size="sm">{$t("review.question.badge")}</Badge>
  </div>

  {#if context}
    <p class="mt-2 text-body-xs text-muted-foreground" data-testid="review-question-context">
      {context}
    </p>
  {/if}

  <div class="mt-3 flex flex-col gap-4">
    {#each fields as field (field.id)}
      {@const options = field.kind === "confirm" ? confirmOptions() : field.options}
      {@const takesOther = field.kind !== "open" && field.kind !== "confirm" && item.allowOther}
      <div class="flex flex-col gap-2" data-testid="review-question-field-{field.id}">
        <p class="text-body-xs font-medium text-foreground">{field.text}</p>
        {#if field.hint}
          <p class="text-micro text-muted-foreground">{field.hint}</p>
        {/if}

        {#if field.kind === "open"}
          <Textarea
            size="sm"
            value={draft.other[field.id] ?? ""}
            oninput={(e) => (draft.other[field.id] = (e.currentTarget as HTMLTextAreaElement).value)}
            {disabled}
            placeholder={$t("review.question.open_placeholder")}
            aria-label={field.text}
            data-testid="review-question-input-{field.id}"
          />
        {:else}
          <p class="text-micro text-muted-foreground">{$t("review.question.proposed")}</p>
          {#if field.kind === "multi"}
            <div class="flex flex-col gap-1.5">
              {#each options as option (option.value)}
                <label class="flex items-center gap-2 text-body-xs text-foreground">
                  <Checkbox
                    checked={(draft.picked[field.id] ?? []).includes(option.value)}
                    onchange={(on) => toggle(field.id, option.value, on)}
                    {disabled}
                    data-testid="review-question-check-{field.id}-{option.value}"
                  />
                  <span>{option.label}</span>
                </label>
              {/each}
            </div>
          {:else}
            <RadioGroup value={(draft.picked[field.id] ?? [])[0] ?? ""}>
              {#each options as option (option.value)}
                <RadioItem
                  value={option.value}
                  checked={(draft.picked[field.id] ?? [])[0] === option.value}
                  onchange={(value) => pickOne(field.id, value)}
                  {disabled}
                  data-testid="review-question-radio-{field.id}-{option.value}"
                >
                  {option.label}
                </RadioItem>
              {/each}
            </RadioGroup>
          {/if}
          {#if takesOther}
            <Textarea
              size="sm"
              value={draft.other[field.id] ?? ""}
              oninput={(e) =>
                (draft.other[field.id] = (e.currentTarget as HTMLTextAreaElement).value)}
              {disabled}
              placeholder={$t("review.question.other_placeholder")}
              aria-label={$t("review.question.other_placeholder")}
              data-testid="review-question-other-{field.id}"
            />
          {/if}
        {/if}
      </div>
    {/each}
  </div>

  <div class="mt-3 flex flex-wrap items-center gap-2">
    <Button
      variant="default"
      size="sm"
      class="h-7 px-3 text-caption"
      disabled={disabled || !ready}
      onclick={submit}
      data-testid="review-question-submit"
    >
      {$t("review.question.submit")}
    </Button>
  </div>

  {#if takesRefusal}
    <form
      class="mt-2 flex items-center gap-2"
      onsubmit={(event) => {
        event.preventDefault();
        if (!disabled) onanswer(refusalRequest(item, reasonText));
      }}
    >
      <Input
        size="sm"
        class="h-7 flex-1 text-caption"
        bind:value={reasonText}
        {disabled}
        maxlength={500}
        placeholder={$t("approval.refuse_reason_placeholder")}
        aria-label={$t("approval.refuse_reason_placeholder")}
        data-testid="review-question-reason"
      />
      <Button
        type="submit"
        variant="outline"
        size="sm"
        class="h-7 px-3 text-caption"
        {disabled}
        data-testid="review-question-refuse"
      >
        {$t("review.question.refuse")}
      </Button>
    </form>
  {/if}
</div>
