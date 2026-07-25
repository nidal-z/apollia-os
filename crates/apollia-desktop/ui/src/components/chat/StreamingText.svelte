<script lang="ts">
  import { renderMarkdown } from "$lib/utils/markdown";
  import "$lib/components/ui/markdown/markdown-prose.css";
  import { t } from "svelte-i18n";
  import { parseStream, isThinking as isThinkingBlocks } from "$lib/chat/streamParser";
  import StreamingCursor from "./StreamingCursor.svelte";

  interface Props {
    text: string;
    /** Stream status - `streaming` shows the cursor, `interrupted` shows an inline marker. */
    status?: "streaming" | "interrupted" | "done";
  }

  let { text, status = "streaming" }: Props = $props();

  const blocks = $derived(parseStream(text));
  const thinkingNow = $derived(isThinkingBlocks(blocks));

  async function handleClick(event: MouseEvent): Promise<void> {
    const target = event.target as HTMLElement;
    const copyBtn = target.closest("[data-copy-code]") as HTMLElement | null;
    if (!copyBtn) return;

    const code = copyBtn.dataset.code;
    if (!code) return;

    try {
      await navigator.clipboard.writeText(decodeURIComponent(code));
    } catch {
      // clipboard API may not be available
    }
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<span class="inline apollia-prose" data-testid="chat-streaming-text" onclick={handleClick}>
  {#each blocks as block, idx (idx)}
    {#if block.type === "thinking"}
      <span class="thinking-segment" data-testid="streaming-thinking-segment">
        <span class="thinking-header">
          {#if !block.closed && thinkingNow && status === "streaming"}
            <span class="thinking-label" role="status" aria-live="polite"
              >{$t("chat.status.thinking")}</span
            >
          {:else}
            <span class="thinking-label">{$t("chat.status.streaming_thought")}</span>
          {/if}
        </span>
        <span class="thinking-content">{@html renderMarkdown(block.content)}</span>
      </span>
    {:else if block.type === "tool"}
      <span class="tool-segment" data-testid="streaming-tool-segment">
        {@html renderMarkdown(block.content)}
      </span>
    {:else}
      {@html renderMarkdown(block.content)}
    {/if}
  {/each}

  {#if status === "streaming"}
    <StreamingCursor />
  {:else if status === "interrupted"}
    <span class="interrupted" data-testid="streaming-interrupted">
      · {$t("chat.status.interrupted")}
    </span>
  {/if}
</span>

<style>
  .thinking-segment {
    display: block;
    margin: 0.375rem 0;
    padding: 0.5rem 0.75rem;
    border-radius: 0.5rem;
    border: 1px solid hsl(var(--primary) / 0.1);
    background-color: hsl(var(--primary) / 0.04);
  }

  .thinking-header {
    display: flex;
    align-items: center;
    margin-bottom: 0.25rem;
  }

  .thinking-label {
    font-size: 10px;
    font-weight: 500;
    color: hsl(var(--primary) / 0.55);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  .thinking-content {
    display: block;
    font-size: 12px;
    font-style: italic;
    color: hsl(var(--muted-foreground) / 0.75);
    font-family: var(--font-mono, ui-monospace, SFMono-Regular, monospace);
  }

  .tool-segment {
    display: inline;
  }

  .interrupted {
    margin-left: 0.25rem;
    font-size: 11px;
    color: hsl(var(--muted-foreground) / 0.7);
    font-style: italic;
  }
</style>
