<script lang="ts" module>
  import type { ChatMessageStyle } from "$lib/stores/chatAppearance";

  /**
   * The frame of a user message: a bubble on the right, or, with the "blocks"
   * style of Appearance, a block the width of the column with no bubble. A
   * message being edited takes the column either way.
   */
  export function userMessageFrame(input: {
    style: ChatMessageStyle;
    editing: boolean;
  }): { align: "items-end" | "items-start"; frame: string } {
    if (input.style === "block") return { align: "items-start", frame: "chat-user-block w-full py-1" };
    return { align: "items-end", frame: input.editing ? "w-full" : "chat-user-bubble" };
  }
</script>

<script lang="ts">
  import { fly } from "svelte/transition";
  import { t, locale } from "svelte-i18n";
  import { getContext } from "svelte";
  import { ANSWER_ACTIONS, type AnswerActionsContext } from "$lib/chat/answerActions";
  import MessageActions from "./MessageActions.svelte";
  import type { ChatMessageView } from "$lib/types";
  import { developerMode } from "$lib/stores/developerMode";
  import MessageRenderer from "./MessageRenderer.svelte";
  import type { Citation } from "$lib/chat/confidenceParser";
  import ReasoningSequence from "./ReasoningSequence.svelte";
  import SourceCards from "./SourceCards.svelte";
  import LinkPreviewList from "./LinkPreviewList.svelte";
  import MessageImages from "./MessageImages.svelte";
  import MessageDocuments from "./MessageDocuments.svelte";
  import { documentsText, typedText } from "$lib/chat/messageDocuments";
  import GenerationInfo from "./GenerationInfo.svelte";
  import { readGeneration, showsGenerationInfo } from "$lib/chat/generationStats";
  import { chatMessageStyle, generationInfoScope } from "$lib/stores/chatAppearance";
  import { parseStream } from "$lib/chat/streamParser";
  import { extractSources } from "$lib/chat/reasoning";
  import {
    parseApolliaActions,
    sanitizeActionButtons,
    executeActionButton,
  } from "$lib/apolliaGuide/actionButtons";

  interface Props {
    message: ChatMessageView;
    sessionId: string;
    /** When false, the bubble is a continuation inside a group - no timestamp footer. */
    showTimestamp?: boolean;
    /**
     * Thread density chosen in Settings > Appearance. "compact" tightens the
     * leading, the bubble padding and the gaps; the width stays the column's.
     */
    variant?: "default" | "compact";
    /** True while a turn is generating - disables regenerate/edit affordances. */
    busy?: boolean;
    /** The answer that keeps its generation figures under "last message only". */
    lastAnswerId?: string | null;
    /** Regenerate the reply to this assistant turn (truncate-in-place). */
    onregenerate?: (messageId: string) => void;
    /** Replace this user message and re-run from it (truncate-in-place). */
    onedit?: (messageId: string, content: string) => void;
  }

  let {
    message,
    sessionId,
    showTimestamp = true,
    variant = "default",
    busy = false,
    lastAnswerId = null,
    onregenerate,
    onedit,
  }: Props = $props();

  const isUser = $derived(message.role === "user");
  // What the user typed: the text of an attached document, which the runtime
  // writes after it, is shown as a chip instead.
  const userText = $derived(typedText(message.content ?? "", message.metadata));
  const isOperator = $derived(!$developerMode);

  const formattedTime = $derived.by(() => {
    const date = new Date(message.created_at);
    return date.toLocaleTimeString($locale ?? "en", { hour: "2-digit", minute: "2-digit" });
  });

  // Split the turn's tool calls: pending/authorized ones drive the always-
  // visible HITL approval cards; the rest form the collapsed activity trace and
  // the source cards.
  const toolCalls = $derived(message.tool_calls ?? []);
  const pendingCalls = $derived(
    toolCalls.filter(
      (c) => c.status === "pending" || c.status === "authorized",
    ),
  );
  const nonPendingCalls = $derived(
    toolCalls.filter(
      (c) => c.status !== "pending" && c.status !== "authorized",
    ),
  );

  // Strip <think>...</think> blocks from rendered content - they are shown via ReasoningSequence.
  const parsedContentBlocks = $derived(parseStream(message.content ?? ""));
  const cleanContent = $derived(
    parsedContentBlocks
      .filter((b) => b.type !== "thinking")
      .map((b) => b.content)
      .join(""),
  );
  const hasInlineThinking = $derived(
    !message.metadata?.thinking_trace &&
      parsedContentBlocks.some((b) => b.type === "thinking" && b.closed),
  );

  // The turn's reasoning and steps (zone 1) show when it thought or acted.
  const hasThinking = $derived(
    !!message.metadata?.thinking_trace || hasInlineThinking,
  );

  // The total of the turn, written by the runtime under its answer: read from
  // the message itself, so a reloaded conversation shows the same line.
  // Which answers show it is a choice of Settings > Appearance.
  const generation = $derived(
    isUser ||
      !showsGenerationInfo({ scope: $generationInfoScope, messageId: message.id, lastAnswerId })
      ? null
      : readGeneration(message.metadata),
  );

  // Source cards (zone 3): deduplicated web pages the agent consulted.
  const sources = $derived(extractSources(nonPendingCalls));

  // The apollia-guide agent may append an ```apollia-actions``` block. Split it
  // out of the visible text and keep the validated navigate/invoke buttons.
  // Only the assistant side carries these; user turns render verbatim.
  const actionSplit = $derived(
    isUser
      ? { text: cleanContent, buttons: [] }
      : parseApolliaActions(cleanContent),
  );
  const displayContent = $derived(actionSplit.text);
  const actionButtons = $derived(sanitizeActionButtons(actionSplit.buttons));

  // A finalized agent turn that produced only reasoning and/or failed tool
  // calls leaves cleanContent empty. ChatMessageBubble only renders committed
  // messages (live tokens go through StreamingMessage), so an empty agent
  // bubble here is a terminated exchange with no user-facing text, never a
  // mid-stream frame. Surface a mode-adapted notice instead of a blank bubble;
  // builder mode points to the reasoning/tool detail already shown above.
  const isEmptyAgentResponse = $derived(
    !isUser && displayContent.trim() === "" && actionButtons.length === 0,
  );
  const emptyResponseLabel = $derived(
    isOperator
      ? $t("chat.empty_response_operator")
      : $t("chat.empty_response_builder"),
  );

  // Flat thread: the reading column constrains width, in both densities.
  const widthClass = "w-full";
  const compact = $derived(variant === "compact");

  // Inline edit. A user turn hands its new text up, and the parent truncates
  // the history from it and runs again; an answer is rewritten in place
  // through the conversation, and nothing runs.
  const answers = getContext<AnswerActionsContext | undefined>(ANSWER_ACTIONS);
  let editing = $state(false);
  let editValue = $state("");

  const canEdit = $derived(
    !!message.content &&
      (isUser ? !!onedit : !!answers && message.role === "assistant"),
  );

  const userFrame = $derived(userMessageFrame({ style: $chatMessageStyle, editing }));

  function startEdit(): void {
    editValue = isUser ? userText : (message.content ?? "");
    editing = true;
  }

  function cancelEdit(): void {
    editing = false;
  }

  function saveEdit(): void {
    const next = editValue.trim();
    if (!next) return;
    editing = false;
    if (!isUser) {
      if (next !== (message.content ?? "").trim()) answers?.editAnswer(message.id, next);
      return;
    }
    // The text of an attached document stays behind the edited text, so the
    // agent still reads it.
    onedit?.(message.id, next + documentsText(message.content ?? "", message.metadata));
  }
</script>

<div
  class="group flex flex-col {isUser ? userFrame.align : 'items-start'} {compact
    ? 'chat-density-compact gap-0.5'
    : 'gap-1'} w-full"
  data-testid="chat-message-{message.id}"
  data-message-style={isUser ? $chatMessageStyle : undefined}
  in:fly={{ y: 4, duration: 200 }}
>
  <!-- Zone 1: the turn in the order it happened, each reasoning fragment a
       foldable block titled by its duration, each action a foldable step.
       Past ten items the list folds behind a "show more" line. -->
  {#if hasThinking || nonPendingCalls.length > 0}
    <div class="{widthClass}">
      <ReasoningSequence
        {message}
        {sessionId}
        {isOperator}
        content={message.content ?? undefined}
        section="timeline"
      />
    </div>
  {/if}

  <!-- Pending HITL approvals stay outside the collapsed strip, always visible. -->
  {#if pendingCalls.length > 0}
    <div class="{widthClass}">
      <ReasoningSequence {message} {sessionId} {isOperator} section="approvals" />
    </div>
  {/if}

  <!-- The documents attached to a user turn sit above its bubble. -->
  {#if isUser}
    <MessageDocuments messageId={message.id} metadata={message.metadata} />
  {/if}

  <div
    class="relative {isUser
      ? userFrame.frame
      : `${widthClass} text-foreground ${compact ? "py-0.5" : "py-1"}`}"
  >
    {#if editing}
        <div class="flex flex-col gap-2">
          <!-- svelte-ignore a11y_autofocus -->
          <textarea
            bind:value={editValue}
            rows="3"
            autofocus
            class="w-full resize-y rounded-md border border-border bg-background px-3 py-2
              text-body-md text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
            data-testid="chat-message-edit-input-{message.id}"
          ></textarea>
          <div class="flex justify-end gap-2">
            <button
              type="button"
              onclick={cancelEdit}
              class="rounded-md px-2.5 py-1 text-body-xs text-muted-foreground
                hover:bg-surface-3 hover:text-foreground transition-colors"
              data-testid="chat-message-edit-cancel-{message.id}"
            >
              {$t("common.cancel")}
            </button>
            <button
              type="button"
              onclick={saveEdit}
              disabled={busy || !editValue.trim()}
              class="rounded-md bg-primary-solid px-2.5 py-1 text-body-xs text-primary-foreground
                hover:opacity-90 transition-opacity disabled:opacity-40 disabled:cursor-not-allowed"
              data-testid="chat-message-edit-save-{message.id}"
            >
              {isUser ? $t("chat.edit_save") : $t("common.save")}
            </button>
          </div>
        </div>
    {:else if isUser}
        <MessageImages {sessionId} messageId={message.id} metadata={message.metadata} />
        {#if userText}
          <p class="whitespace-pre-wrap break-words">{userText}</p>
        {/if}
    {:else if isEmptyAgentResponse}
      <p
        class="whitespace-pre-wrap break-words text-body-md italic text-muted-foreground/80"
        data-testid="chat-empty-response-{message.id}"
      >
        {emptyResponseLabel}
      </p>
    {:else}
      {#if displayContent.trim() !== ""}
        <!-- Zone 3: the answer, the hero. -->
        <div class="chat-answer">
          <MessageRenderer
            content={displayContent}
            citations={(message.metadata?.citations as Citation[] | undefined) ?? []}
          />
        </div>
        <LinkPreviewList content={displayContent} />
      {/if}
      <!-- Zone 4: source cards for the web pages consulted. -->
      {#if sources.length > 0}
        <SourceCards {sources} />
      {/if}
      {#if actionButtons.length > 0}
        <div
          class="mt-2 flex flex-wrap gap-2"
          data-testid="chat-action-buttons-{message.id}"
        >
          {#each actionButtons as btn (btn.action + btn.target + btn.label)}
            <button
              type="button"
              class="rounded-lg border border-border bg-surface-2 px-3 py-1.5 text-body-sm
                font-medium text-foreground hover:bg-surface-3 hover:text-primary
                focus-visible:outline focus-visible:outline-2 focus-visible:outline-ring
                transition-colors"
              onclick={() => void executeActionButton(btn)}
            >
              {btn.label}
            </button>
          {/each}
        </div>
      {/if}
    {/if}

    {#if showTimestamp}
      <p class="mt-1 text-micro text-left text-muted-foreground/50">
        {formattedTime}
      </p>
    {/if}
  </div>

  <!-- The generation figures of the turn, and the detail per call on demand. -->
  {#if generation}
    <div class={widthClass}>
      <GenerationInfo
        {generation}
        {sessionId}
        turnId={message.id}
        stepCount={nonPendingCalls.length}
      />
    </div>
  {/if}

  <!-- Zone 5: the actions of the turn, a row below it. -->
  {#if !editing && (isUser ? canEdit || !!answers : !!message.content || !!answers)}
    <MessageActions
      {message}
      copyText={isUser ? userText : (message.content ?? "")}
      {busy}
      onregenerate={isUser ? undefined : onregenerate}
      onstartedit={canEdit ? startEdit : undefined}
    />
  {/if}
</div>
