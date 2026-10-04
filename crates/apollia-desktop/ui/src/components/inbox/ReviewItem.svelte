<script lang="ts">
  /**
   * One line of the "To review" list: where the decision comes from, with the
   * way back to it, then its card. A consent, the pause of a task and a batch
   * item show the decision card of the chat; a question shows the question
   * card. Once answered, the line shrinks to the answer and offers "Cancel"
   * until it leaves.
   */
  import { t } from "svelte-i18n";
  import { Check } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { developerMode } from "$lib/stores/developerMode";
  import type { DecideRequest, PendingDecision } from "$lib/ipc/pendingDecisions";
  import ApprovalCard from "../chat/ApprovalCard.svelte";
  import OperatorApprovalCard from "../chat/OperatorApprovalCard.svelte";
  import type { ConsentDecision } from "../chat/ConsentAnswers.svelte";
  import { liveApprovalCall } from "../chat/liveApprovalCall";
  import ReviewQuestionCard from "./ReviewQuestionCard.svelte";
  import {
    cardAnswers,
    collapsedCommand,
    consentRequest,
    isQuestion,
    originOf,
  } from "./reviewModel";

  interface Props {
    item: PendingDecision;
    /** The title of the conversation the item comes from, when it has one. */
    conversationTitle?: string | null;
    /** The answer given and not sent yet, `null` while the item waits. */
    held?: DecideRequest | null;
    relTime: (iso: string) => string;
    onanswer: (item: PendingDecision, request: DecideRequest) => void;
    oncancel: (item: PendingDecision) => void;
    onopen: (item: PendingDecision) => void;
  }

  let {
    item,
    conversationTitle = null,
    held = null,
    relTime,
    onanswer,
    oncancel,
    onopen,
  }: Props = $props();

  const origin = $derived(originOf(item));
  const agentLabel = $derived(item.agentId ?? $t("review.agent_unknown"));
  const toolName = $derived(item.access ?? "agent_action");

  const originText = $derived.by(() => {
    switch (origin.kind) {
      case "conversation":
        return conversationTitle
          ? $t("review.origin.conversation_named", { values: { title: conversationTitle } })
          : $t("review.origin.conversation");
      case "task":
        return $t("review.origin.task");
      case "batch":
        return $t("review.origin.batch", { values: { position: origin.position } });
      default:
        return $t("review.origin.unknown");
    }
  });
  const openKey = $derived(
    origin.kind === "conversation"
      ? "review.open.conversation"
      : origin.kind === "task"
        ? "review.open.task"
        : null,
  );

  // A long command is folded in the list: its first lines, then the whole of it
  // on request, and the answers only once it is whole on screen.
  const command = $derived(item.code ? collapsedCommand(item.code) : null);
  let unfolded = $state(false);
  const folded = $derived(command !== null && command.collapsed && !unfolded);

  // The default card describes a tool call by its tool. The pause of a task
  // and a batch item say what they wait for in a sentence, shown above it.
  const showsSummary = $derived(item.kind !== "consent" && item.summary.trim().length > 0);

  async function decide(decision: ConsentDecision): Promise<void> {
    onanswer(item, consentRequest(item, decision));
  }
</script>

<article
  class="flex flex-col gap-1"
  data-testid="review-item"
  data-kind={item.kind}
  data-item-id={item.id}
>
  <header class="flex flex-wrap items-center gap-x-3 gap-y-1 text-caption text-muted-foreground">
    <span data-testid="review-origin">{originText}</span>
    {#if openKey && !held}
      <Button
        variant="link"
        size="sm"
        class="h-auto px-0 text-caption"
        onclick={() => onopen(item)}
        data-testid="review-open"
      >
        {$t(openKey)}
      </Button>
    {/if}
    {#if item.askedAt}
      <span class="ml-auto tabular-nums">{relTime(item.askedAt)}</span>
    {/if}
  </header>

  {#if held}
    <div
      class="my-1.5 flex flex-col gap-2 rounded-lg border border-border/60 border-l-2 border-l-success bg-surface-1 px-3 py-2.5"
      data-testid="review-answered"
    >
      <div class="flex items-center gap-2 text-body-xs font-medium text-foreground">
        <Check class="h-3.5 w-3.5 text-success" />
        <span>
          {$t(`review.answered.${held.answer}`, { values: { agent: agentLabel } })}
        </span>
      </div>
      <div class="flex flex-wrap items-center gap-3 text-caption text-muted-foreground">
        <span>{$t("review.answered.recorded")}</span>
        <Button
          variant="outline"
          size="sm"
          class="h-7 px-3 text-caption"
          onclick={() => oncancel(item)}
          data-testid="review-cancel"
        >
          {$t("review.answered.cancel")}
        </Button>
      </div>
    </div>
  {:else if isQuestion(item)}
    <ReviewQuestionCard {item} {agentLabel} onanswer={(request) => onanswer(item, request)} />
  {:else if folded && command}
    <div
      class="my-1.5 rounded-lg border border-border/60 border-l-2 border-l-warning bg-surface-1 px-3 py-2.5 text-xs"
      data-testid="review-folded"
    >
      <p class="text-sm font-medium text-foreground">
        {$t("review.asks_consent", { values: { agent: agentLabel } })}
      </p>
      <pre
        class="mt-2 overflow-x-auto whitespace-pre rounded-lg bg-surface-2 p-2 font-mono text-micro text-foreground">{command.text}</pre>
      <p class="mt-2 text-micro text-muted-foreground">
        {$t("review.code.collapsed", { values: { shown: command.shown, total: command.total } })}
      </p>
      <Button
        variant="link"
        size="sm"
        class="mt-1 h-auto px-0 text-caption"
        onclick={() => (unfolded = true)}
        data-testid="review-unfold"
      >
        {$t("review.code.show_all")}
      </Button>
    </div>
  {:else if $developerMode}
    <ApprovalCard
      sessionId={item.origin.sessionId ?? ""}
      messageId={item.origin.messageId ?? ""}
      toolCallId={item.origin.toolCallId ?? item.id}
      {toolName}
      inputPreview={item.summary}
      answers={cardAnswers(item)}
      detail={item.detail ?? ""}
      code={item.code ?? null}
      workingDir={item.workingDir ?? null}
      ondecide={decide}
    />
  {:else}
    {#if showsSummary}
      <p class="mt-1 text-body-xs text-foreground" data-testid="review-summary">
        {item.summary}
      </p>
    {/if}
    <OperatorApprovalCard
      sessionId={item.origin.sessionId ?? ""}
      messageId={item.origin.messageId ?? ""}
      toolCallId={item.origin.toolCallId ?? item.id}
      toolCall={liveApprovalCall(toolName, item.summary, item.detail)}
      answers={cardAnswers(item)}
      code={item.code ?? null}
      workingDir={item.workingDir ?? null}
      ondecide={decide}
    />
  {/if}
</article>
