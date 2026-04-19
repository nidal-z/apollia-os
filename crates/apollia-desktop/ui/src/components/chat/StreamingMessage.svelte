<!--
  StreamingMessage — isolates the streaming assistant bubble (US-SP42-035 / B.66).

  Rationale: keeping the streaming text in its own component means that every
  token append re-evaluates only this subtree.  The committed `ChatMessageBubble`
  list stays reactively stable because it depends on the immutable `messages`
  array, not on `tokenBuffer`.
-->
<script lang="ts">
  import StreamingText from "./StreamingText.svelte";

  interface Props {
    /** Accumulated streamed text (per-session buffer snapshot). */
    text: string;
    /** Session mode — bubble only shown in "libre" mode. */
    sessionMode: "libre" | "agent";
  }

  let { text, sessionMode }: Props = $props();
</script>

{#if sessionMode === "libre"}
  <div class="flex justify-start" data-testid="chat-message-streaming">
    <div
      class="max-w-[min(82ch,92%)] lg:max-w-[min(78ch,80%)] rounded-2xl rounded-bl-sm
        bg-card/80 border border-border/40 px-3.5 py-2.5 text-[13px] text-foreground
        shadow-[0_2px_10px_-4px_rgba(0,0,0,0.08)]"
    >
      <StreamingText {text} />
    </div>
  </div>
{/if}
