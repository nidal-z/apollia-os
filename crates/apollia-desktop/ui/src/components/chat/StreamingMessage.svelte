<!--
  StreamingMessage - isolates the streaming assistant bubble.

  Thinking blocks are rendered ABOVE the bubble (matching the committed-message
  ReasoningSequence layout) so they are never confused with the response text.
  An unclosed thinking block (active reasoning) shows a ThinkingBadge above the
  bubble; closed blocks are shown as collapsed thinking cards.
-->
<script lang="ts">
  import { t } from "svelte-i18n";
  import StreamingText from "./StreamingText.svelte";
  import ThinkingBadge from "./ThinkingBadge.svelte";
  import { parseStream, isThinking as isActiveThinking } from "$lib/chat/streamParser";

  interface Props {
    /** Accumulated streamed text (per-session buffer snapshot). */
    text: string;
    /** Session mode - bubble only shown in "libre" mode. */
    sessionMode: "libre" | "agent";
  }

  let { text, sessionMode }: Props = $props();

  const blocks = $derived(parseStream(text));
  const activeThinking = $derived(isActiveThinking(blocks));
  const closedThinkingBlocks = $derived(blocks.filter((b) => b.type === "thinking" && b.closed));
  // Only non-thinking content goes into the bubble
  const textContent = $derived(
    blocks
      .filter((b) => b.type !== "thinking")
      .map((b) => b.content)
      .join(""),
  );
</script>

{#if sessionMode === "libre"}
  <div class="flex flex-col items-start gap-1" data-testid="chat-message-streaming">
    <!-- Thinking blocks above the bubble -->
    {#if activeThinking || closedThinkingBlocks.length > 0}
      <div
        class="max-w-[min(82ch,92%)] lg:max-w-[min(78ch,80%)] space-y-1"
        data-testid="streaming-thinking-area"
      >
        {#if activeThinking}
          <div class="rounded-lg border border-primary/10 bg-primary/[0.04] px-3 py-2">
            <ThinkingBadge />
          </div>
        {/if}
        {#each closedThinkingBlocks as block, i (i)}
          <div
            class="rounded-lg border border-primary/10 bg-primary/[0.04] px-3 py-2"
            data-testid="streaming-closed-thinking-{i}"
          >
            <span class="block text-[10px] font-medium uppercase tracking-wider text-primary/55 mb-1">
              {$t("chat.status.streaming_thought")}
            </span>
            <span class="block text-[12px] italic text-muted-foreground/75 font-mono leading-snug line-clamp-3">
              {block.content}
            </span>
          </div>
        {/each}
      </div>
    {/if}

    <!-- Main response bubble (thinking-free) -->
    {#if textContent || !activeThinking}
      <div
        class="max-w-[min(82ch,92%)] lg:max-w-[min(78ch,80%)] rounded-2xl rounded-bl-sm
          bg-card/80 border border-border/40 px-3.5 py-2.5 text-[13px] text-foreground
          shadow-elev-1"
      >
        <StreamingText text={textContent} />
      </div>
    {/if}
  </div>
{/if}
