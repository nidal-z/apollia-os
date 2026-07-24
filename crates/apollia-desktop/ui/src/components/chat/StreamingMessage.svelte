<!--
  StreamingMessage - isolates the streaming assistant bubble.

  Thinking blocks are rendered ABOVE the bubble (matching the committed-message
  ReasoningSequence layout) so they are never confused with the response text.
  An unclosed thinking block (active reasoning) shows a ThinkingBadge above the
  bubble; closed blocks are shown as collapsed thinking cards.
-->
<script lang="ts">
  import { t } from "svelte-i18n";
  import { fly } from "svelte/transition";
  import { Avatar } from "$lib/components/ui/avatar";
  import StreamingText from "./StreamingText.svelte";
  import ThinkingBadge from "./ThinkingBadge.svelte";
  import { parseStream, isThinking as isActiveThinking } from "$lib/chat/streamParser";

  interface Props {
    /** Accumulated streamed text (per-session buffer snapshot). */
    text: string;
    /** Session mode - bubble only shown in "libre" mode. */
    sessionMode: "libre" | "agent";
    /** Assistant display name for the turn header. */
    agentName?: string | null;
  }

  let { text, sessionMode, agentName = null }: Props = $props();

  const displayName = $derived(agentName ?? $t("chat.assistant", { default: "Assistant" }));

  const blocks = $derived(parseStream(text));
  const activeThinking = $derived(isActiveThinking(blocks));
  const closedThinkingBlocks = $derived(blocks.filter((b) => b.type === "thinking" && b.closed));
  // Live reasoning text for the open thinking block, streamed token by token so
  // the user sees the model think in real time instead of a static badge.
  const activeThinkingContent = $derived(
    activeThinking ? (blocks.at(-1)?.content ?? "") : "",
  );
  // Only non-thinking content goes into the bubble
  const textContent = $derived(
    blocks
      .filter((b) => b.type !== "thinking")
      .map((b) => b.content)
      .join(""),
  );
</script>

<!-- Rendered in both libre and agent mode so an agent turn also shows its
     live reasoning and text instead of a bare "thinking" placeholder while a
     slow local completion runs. -->
<div class="flex flex-col items-start" data-testid="chat-message-streaming" data-mode={sessionMode}>
  <div class="flex items-center gap-2 mb-1.5 px-0.5">
    <Avatar name={displayName} size="xs" ring={false} />
    <span class="text-[12px] font-medium text-muted-foreground">{displayName}</span>
  </div>

  <!-- Reasoning as flat captions (not bubbles): a thin left rule + muted
       italic text keeps it a side note in the flat thread. Active reasoning
       streams token by token; closed blocks collapse to a short caption. -->
  {#if activeThinking || closedThinkingBlocks.length > 0}
    <div
      class="w-full space-y-1.5 border-l-2 border-border/40 pl-2.5"
      data-testid="streaming-thinking-area"
    >
      {#if activeThinking}
        <div class="flex flex-col gap-0.5">
          <ThinkingBadge />
          {#if activeThinkingContent.trim()}
            <span
              class="block max-h-40 overflow-hidden whitespace-pre-wrap text-[12px] italic leading-snug text-muted-foreground/70"
              data-testid="streaming-active-thinking"
            >
              {activeThinkingContent}
            </span>
          {/if}
        </div>
      {/if}
      {#each closedThinkingBlocks as block, i (i)}
        <div
          class="flex flex-col gap-0.5"
          data-testid="streaming-closed-thinking-{i}"
          in:fly={{ x: -8, duration: 220 }}
        >
          <span class="block text-[10px] font-medium uppercase tracking-wider text-muted-foreground/50">
            {$t("chat.status.streaming_thought")}
          </span>
          <span class="block text-[12px] italic text-muted-foreground/70 leading-snug line-clamp-3">
            {block.content}
          </span>
        </div>
      {/each}
    </div>
  {/if}

  <!-- Main response (thinking-free), transparent in the flat thread -->
  {#if textContent || !activeThinking}
    <div class="w-full py-1 text-[14px] text-foreground">
      <StreamingText text={textContent} />
    </div>
  {/if}
</div>
