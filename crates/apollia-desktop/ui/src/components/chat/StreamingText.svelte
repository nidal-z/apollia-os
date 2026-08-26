<script lang="ts">
  import { decorateCodeBlocks, handleMarkdownLinkClick, renderMarkdown } from "$lib/utils/markdown";
  import "$lib/components/ui/markdown/markdown-prose.css";
  import { t } from "svelte-i18n";
  import StreamingCursor from "./StreamingCursor.svelte";

  interface Props {
    /**
     * Answer text, already stripped of stream markers by `answerText`. Parsing
     * it again here would rescan the whole accumulated answer on every chunk for
     * markers that cannot be present.
     */
    text: string;
    /** Stream status - `streaming` shows the cursor, `interrupted` shows an inline marker. */
    status?: "streaming" | "interrupted" | "done";
  }

  let { text, status = "streaming" }: Props = $props();

  let container = $state<HTMLElement | undefined>();

  // Graft the code-block chrome (wrapper, language label, copy button) onto
  // the sanitized markup after each streaming chunk re-renders it.
  // `renderMarkdown` emits rendering tags only, so the button a click on
  // `[data-copy-code]` finds below can only be one this decoration created,
  // with `data-code` derived from the displayed text.
  $effect(() => {
    void text; // re-run on every chunk
    if (!container) return;
    decorateCodeBlocks(container);
  });

  async function handleClick(event: MouseEvent): Promise<void> {
    // Outbound links first: the anchors are injected markup, so this
    // container is the only place that can route them to the opener. The
    // copy-button branch below returns early and would swallow the click.
    if (handleMarkdownLinkClick(event)) return;

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
<span bind:this={container} class="inline apollia-prose" data-testid="chat-streaming-text" onclick={handleClick}>
  {@html renderMarkdown(text)}

  {#if status === "streaming"}
    <StreamingCursor />
  {:else if status === "interrupted"}
    <span class="interrupted" data-testid="streaming-interrupted">
      · {$t("chat.status.interrupted")}
    </span>
  {/if}
</span>

<style>
  .interrupted {
    margin-left: 0.25rem;
    font-size: var(--text-caption);
    color: hsl(var(--muted-foreground) / 0.7);
    font-style: italic;
  }
</style>
