<script lang="ts">
  import { fly } from "svelte/transition";
  import { t } from "svelte-i18n";
  import { Copy, Check, RefreshCw, Pencil } from "lucide-svelte";
  import type { ChatMessageView } from "$lib/types";
  import { uiMode } from "$lib/stores/mode";
  import MessageRenderer from "./MessageRenderer.svelte";
  import type { Citation } from "$lib/chat/confidenceParser";
  import ReasoningSequence from "./ReasoningSequence.svelte";
  import ActivityStrip from "./ActivityStrip.svelte";
  import SourceCards from "./SourceCards.svelte";
  import LinkPreviewList from "./LinkPreviewList.svelte";
  import { parseStream } from "$lib/chat/streamParser";
  import { extractSources, sumToolDurationMs, reasoningFragmentCount } from "$lib/chat/reasoning";
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
    /** Visual density. "compact" clamps the max-width at 72 % for embedded contexts. */
    variant?: "default" | "compact";
    /** True while a turn is generating - disables regenerate/edit affordances. */
    busy?: boolean;
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
    onregenerate,
    onedit,
  }: Props = $props();

  const isUser = $derived(message.role === "user");
  const isOperator = $derived($uiMode === "operator");

  const formattedTime = $derived.by(() => {
    const date = new Date(message.created_at);
    return date.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
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

  // Reasoning strip input (zone 1). The strip exists only when the turn
  // produced reasoning; finalized tool calls now render as their own visible
  // rows in the flow (zone 2), not inside the strip.
  const hasThinking = $derived(
    !!message.metadata?.thinking_trace || hasInlineThinking,
  );
  const activityDurationMs = $derived(sumToolDurationMs(nonPendingCalls));

  // Counts surfaced in the collapsed strip summary so the reader knows how much
  // reasoning and how many tool calls the turn folded away.
  const reasoningCount = $derived(
    reasoningFragmentCount(
      { id: message.id, tool_calls: nonPendingCalls, metadata: message.metadata },
      message.content ?? undefined,
    ),
  );
  const toolCount = $derived(nonPendingCalls.length);

  // Source cards (zone 3): deduplicated web pages the agent consulted. Their
  // count also feeds the strip summary so both zones agree.
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

  // Flat thread: the reading column constrains width. Compact embeds still cap.
  const widthClass = $derived(
    variant === "compact" ? "max-w-[min(78ch,72%)]" : "w-full",
  );

  let copied = $state(false);

  async function handleCopy(): Promise<void> {
    if (!message.content) return;
    try {
      await navigator.clipboard.writeText(message.content);
      copied = true;
      setTimeout(() => { copied = false; }, 1500);
    } catch {
      // clipboard API may not be available
    }
  }

  // Inline edit of a user turn (G10). Opening the editor seeds it with the
  // current text; saving hands the new content up so the parent truncates the
  // history from this message and re-runs.
  let editing = $state(false);
  let editValue = $state("");

  const canRegenerate = $derived(!isUser && !!onregenerate && !!message.content);
  const canEdit = $derived(isUser && !!onedit && !!message.content);

  function startEdit(): void {
    editValue = message.content ?? "";
    editing = true;
  }

  function cancelEdit(): void {
    editing = false;
  }

  function saveEdit(): void {
    const next = editValue.trim();
    if (!next) return;
    editing = false;
    onedit?.(message.id, next);
  }
</script>

<div
  class="group flex flex-col {isUser ? 'items-end' : 'items-start'} gap-1 w-full"
  data-testid="chat-message-{message.id}"
  in:fly={{ y: 4, duration: 200 }}
>
  <!-- Zone 1: quiet reasoning strip - thoughts only, as flat narrated captions,
       collapsed by default in both modes. -->
  {#if hasThinking}
    <div class="{widthClass}">
      <ActivityStrip durationMs={activityDurationMs} {reasoningCount} {toolCount}>
        <ReasoningSequence
          {message}
          {sessionId}
          {isOperator}
          content={message.content ?? undefined}
          section="reasoning"
        />
      </ActivityStrip>
    </div>
  {/if}

  <!-- Zone 2: the tool calls, visible in the thread flow. Each row expands to
       its bespoke per-tool body (operator abstraction / builder raw). -->
  {#if nonPendingCalls.length > 0}
    <div class="{widthClass}">
      <ReasoningSequence
        {message}
        {sessionId}
        {isOperator}
        content={message.content ?? undefined}
        section="tools"
      />
    </div>
  {/if}

  <!-- Pending HITL approvals stay outside the collapsed strip, always visible. -->
  {#if pendingCalls.length > 0}
    <div class="{widthClass}">
      <ReasoningSequence {message} {sessionId} {isOperator} section="approvals" />
    </div>
  {/if}

  <div
    class="relative {isUser
      ? editing
        ? widthClass
        : 'chat-user-bubble'
      : `${widthClass} text-foreground py-1`}"
  >
    {#if isUser}
      {#if editing}
        <div class="flex flex-col gap-2">
          <!-- svelte-ignore a11y_autofocus -->
          <textarea
            bind:value={editValue}
            rows="3"
            autofocus
            class="w-full resize-y rounded-md border border-border bg-background px-3 py-2
              text-[14px] text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
            data-testid="chat-message-edit-input-{message.id}"
          ></textarea>
          <div class="flex justify-end gap-2">
            <button
              type="button"
              onclick={cancelEdit}
              class="rounded-md px-2.5 py-1 text-[12px] text-muted-foreground
                hover:bg-surface-3 hover:text-foreground transition-colors"
              data-testid="chat-message-edit-cancel-{message.id}"
            >
              {$t("common.cancel")}
            </button>
            <button
              type="button"
              onclick={saveEdit}
              disabled={busy || !editValue.trim()}
              class="rounded-md bg-primary-solid px-2.5 py-1 text-[12px] text-primary-foreground
                hover:opacity-90 transition-opacity disabled:opacity-40 disabled:cursor-not-allowed"
              data-testid="chat-message-edit-save-{message.id}"
            >
              {$t("chat.edit_save")}
            </button>
          </div>
        </div>
      {:else if message.content}
        <p class="whitespace-pre-wrap break-words">{message.content}</p>
      {/if}
    {:else if isEmptyAgentResponse}
      <p
        class="whitespace-pre-wrap break-words text-[14px] italic text-muted-foreground/80"
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
              class="rounded-lg border border-border bg-surface-2 px-3 py-1.5 text-[13px]
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
      <p class="mt-1 text-[10px] text-left text-muted-foreground/50">
        {formattedTime}
      </p>
    {/if}
  </div>

  <!-- Zone 5: turn actions - a row below the turn (mockup .actions). Assistant
       turns get regenerate + copy; user turns get the inline edit affordance. -->
  {#if isUser}
    {#if canEdit && !editing}
      <div class="chat-actions" data-testid="chat-message-actions-{message.id}">
        <button
          type="button"
          onclick={startEdit}
          disabled={busy}
          title={$t("chat.edit_message")}
          aria-label={$t("chat.edit_message")}
          data-testid="chat-message-edit-{message.id}"
        >
          <Pencil size={15} />
        </button>
      </div>
    {/if}
  {:else if message.content}
    <div class="chat-actions" data-testid="chat-message-actions-{message.id}">
      {#if canRegenerate}
        <button
          type="button"
          onclick={() => onregenerate?.(message.id)}
          disabled={busy}
          title={$t("chat.regenerate")}
          aria-label={$t("chat.regenerate")}
          data-testid="chat-message-regenerate-{message.id}"
        >
          <RefreshCw size={15} />
        </button>
      {/if}
      <button
        type="button"
        onclick={handleCopy}
        title={$t("chat.copy_message")}
        aria-label={$t("chat.copy_message")}
        data-testid="chat-message-copy-{message.id}"
      >
        {#if copied}
          <Check size={15} class="text-success" />
        {:else}
          <Copy size={15} />
        {/if}
      </button>
    </div>
  {/if}
</div>
