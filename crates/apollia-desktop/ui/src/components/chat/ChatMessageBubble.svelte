<script lang="ts">
  import { fly } from "svelte/transition";
  import { t } from "svelte-i18n";
  import { Copy, Check } from "lucide-svelte";
  import type { ChatMessageView } from "$lib/types";
  import { uiMode } from "$lib/stores/mode";
  import MessageRenderer from "./MessageRenderer.svelte";
  import type { Citation } from "$lib/chat/confidenceParser";
  import ReasoningSequence from "./ReasoningSequence.svelte";
  import LinkPreviewList from "./LinkPreviewList.svelte";
  import { parseStream } from "$lib/chat/streamParser";

  interface Props {
    message: ChatMessageView;
    sessionId: string;
    /** When false, the bubble is a continuation inside a group - no timestamp footer. */
    showTimestamp?: boolean;
    /** Visual density. "compact" clamps the max-width at 72 % for embedded contexts. */
    variant?: "default" | "compact";
  }

  let {
    message,
    sessionId,
    showTimestamp = true,
    variant = "default",
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

  // A finalized agent turn that produced only reasoning and/or failed tool
  // calls leaves cleanContent empty. ChatMessageBubble only renders committed
  // messages (live tokens go through StreamingMessage), so an empty agent
  // bubble here is a terminated exchange with no user-facing text, never a
  // mid-stream frame. Surface a mode-adapted notice instead of a blank bubble;
  // builder mode points to the reasoning/tool detail already shown above.
  const isEmptyAgentResponse = $derived(
    !isUser && cleanContent.trim() === "",
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
    class="relative {widthClass} text-[14px] leading-relaxed {blockClass} {!isUser && message.content ? 'pr-10' : ''}"
  >
    <!-- Copy button - floating, backdrop-blur, always reachable on touch. -->
    {#if message.content && !isUser}
      <button
        onclick={handleCopy}
        class="absolute top-2 right-2 z-10 h-6 w-6 rounded-md flex items-center justify-center
          bg-card/70 backdrop-blur-sm border border-border/40 text-muted-foreground/60
          opacity-0 group-hover:opacity-100 focus-visible:opacity-100
          hover:text-foreground hover:bg-card/90 transition-all shadow-sm
          supports-[hover:none]:opacity-100"
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
    {/if}

    {#if isUser}
      {#if message.content}
        <p class="whitespace-pre-wrap break-words">{message.content}</p>
      {/if}
    {:else if isEmptyAgentResponse}
      <p
        class="whitespace-pre-wrap break-words italic text-muted-foreground/80"
        data-testid="chat-empty-response-{message.id}"
      >
        {emptyResponseLabel}
      </p>
    {:else}
      <MessageRenderer
        content={cleanContent}
        citations={(message.metadata?.citations as Citation[] | undefined) ?? []}
      />
      <LinkPreviewList content={cleanContent} />
    {/if}

    {#if showTimestamp}
      <p class="mt-1 text-[10px] text-left text-muted-foreground/50">
        {formattedTime}
      </p>
    {/if}
  </div>
</div>
