<script lang="ts">
  import { fly } from "svelte/transition";
  import { t } from "svelte-i18n";
  import { Copy, Check, RefreshCw, Pencil } from "lucide-svelte";
  import type { ChatMessageView } from "$lib/types";
  import { uiMode } from "$lib/stores/mode";
  import MessageRenderer from "./MessageRenderer.svelte";
  import type { Citation } from "$lib/chat/confidenceParser";
  import ReasoningSequence from "./ReasoningSequence.svelte";
  import LinkPreviewList from "./LinkPreviewList.svelte";
  import { parseStream } from "$lib/chat/streamParser";
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

  const hasToolCalls = $derived(
    message.tool_calls !== null && message.tool_calls.length > 0,
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

  // Flat thread: the user turn sits in a light surface block, the assistant
  // turn renders transparently in the same column. No bubble, no gradient.
  const blockClass = $derived(
    isUser
      ? "bg-surface-2 text-foreground rounded-xl px-4 py-3"
      : "text-foreground py-1",
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
  class="group flex flex-col items-start gap-1 w-full"
  data-testid="chat-message-{message.id}"
  in:fly={{ y: 4, duration: 200 }}
>
  {#if hasToolCalls || message.metadata?.thinking_trace || hasInlineThinking}
    <div class="{widthClass}">
      <ReasoningSequence {message} {sessionId} {isOperator} content={message.content ?? undefined} />
    </div>
  {/if}

  <div
    class="relative {widthClass} text-[14px] leading-relaxed {blockClass} {!isUser && message.content ? (canRegenerate ? 'pr-16' : 'pr-10') : ''} {canEdit && !editing ? 'pr-10' : ''}"
  >
    <!-- Copy / regenerate - floating, backdrop-blur, always reachable on touch. -->
    {#if message.content && !isUser}
      <div
        class="absolute top-2 right-2 z-10 flex items-center gap-1
          opacity-0 group-hover:opacity-100 focus-within:opacity-100
          supports-[hover:none]:opacity-100"
      >
        {#if canRegenerate}
          <button
            onclick={() => onregenerate?.(message.id)}
            disabled={busy}
            class="h-6 w-6 rounded-md flex items-center justify-center
              bg-card/70 backdrop-blur-sm border border-border/40 text-muted-foreground/60
              hover:text-foreground hover:bg-card/90 transition-all shadow-sm
              disabled:opacity-40 disabled:cursor-not-allowed"
            title={$t("chat.regenerate")}
            data-testid="chat-message-regenerate-{message.id}"
            aria-label={$t("chat.regenerate")}
          >
            <RefreshCw size={11} />
          </button>
        {/if}
        <button
          onclick={handleCopy}
          class="h-6 w-6 rounded-md flex items-center justify-center
            bg-card/70 backdrop-blur-sm border border-border/40 text-muted-foreground/60
            hover:text-foreground hover:bg-card/90 transition-all shadow-sm"
          title={$t("chat.copy_message")}
          data-testid="chat-message-copy-{message.id}"
          aria-label={$t("chat.copy_message")}
        >
          {#if copied}
            <Check size={11} class="text-success" />
          {:else}
            <Copy size={11} />
          {/if}
        </button>
      </div>
    {/if}

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
        {#if canEdit}
          <button
            type="button"
            onclick={startEdit}
            disabled={busy}
            class="absolute top-2 right-2 z-10 h-6 w-6 rounded-md flex items-center justify-center
              bg-card/70 backdrop-blur-sm border border-border/40 text-muted-foreground/60
              opacity-0 group-hover:opacity-100 focus-visible:opacity-100
              hover:text-foreground hover:bg-card/90 transition-all shadow-sm
              supports-[hover:none]:opacity-100
              disabled:opacity-40 disabled:cursor-not-allowed"
            title={$t("chat.edit_message")}
            data-testid="chat-message-edit-{message.id}"
            aria-label={$t("chat.edit_message")}
          >
            <Pencil size={11} />
          </button>
        {/if}
      {/if}
    {:else if isEmptyAgentResponse}
      <p
        class="whitespace-pre-wrap break-words italic text-muted-foreground/80"
        data-testid="chat-empty-response-{message.id}"
      >
        {emptyResponseLabel}
      </p>
    {:else}
      {#if displayContent.trim() !== ""}
        <MessageRenderer
          content={displayContent}
          citations={(message.metadata?.citations as Citation[] | undefined) ?? []}
        />
        <LinkPreviewList content={displayContent} />
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
</div>
