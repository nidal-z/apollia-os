<script lang="ts">
  import { renderMarkdown } from "$lib/utils/markdown";
  import "$lib/components/ui/markdown/markdown-prose.css";
  import { BrainCircuit } from "lucide-svelte";
  import { t } from "svelte-i18n";

  interface Props {
    text: string;
  }

  let { text }: Props = $props();

  /** Split text into segments: thinking blocks and regular content. */
  const segments = $derived.by(() => {
    const result: { type: "think" | "text"; content: string }[] = [];
    let cursor = 0;
    const src = text;
    const tagOpen = "<think>";
    const tagClose = "</think>";

    while (cursor < src.length) {
      const start = src.indexOf(tagOpen, cursor);
      if (start === -1) {
        // No more think blocks — rest is regular text.
        const rest = src.slice(cursor);
        if (rest) result.push({ type: "text", content: rest });
        break;
      }
      // Push text before <think>
      if (start > cursor) {
        result.push({ type: "text", content: src.slice(cursor, start) });
      }
      const end = src.indexOf(tagClose, start + tagOpen.length);
      if (end === -1) {
        // Unclosed <think> — still streaming thinking content.
        const thinkContent = src.slice(start + tagOpen.length);
        if (thinkContent) result.push({ type: "think", content: thinkContent });
        cursor = src.length;
      } else {
        const thinkContent = src.slice(start + tagOpen.length, end);
        if (thinkContent) result.push({ type: "think", content: thinkContent });
        cursor = end + tagClose.length;
      }
    }
    return result;
  });

  /** Whether we're currently inside a thinking block (unclosed). */
  const isThinking = $derived(
    text.includes("<think>") &&
    (text.lastIndexOf("<think>") > text.lastIndexOf("</think>"))
  );

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
  {#each segments as segment (segment)}
    {#if segment.type === "think"}
      <div class="my-1.5 rounded-lg border border-primary/10 bg-primary/5 px-3 py-2">
        <div class="mb-1 flex items-center gap-1.5">
          <BrainCircuit size={12} class="text-primary/50" />
          <span class="text-[10px] font-medium text-primary/50">
            {isThinking && segment === segments[segments.length - 1] ? $t("chat.thinking") : $t("chat.streaming_thought")}
          </span>
        </div>
        <span class="text-[12px] italic text-muted-foreground/70">{@html renderMarkdown(segment.content)}</span>
      </div>
    {:else}
      {@html renderMarkdown(segment.content)}
    {/if}
  {/each}
  <span class="inline-block w-0.5 h-4 bg-primary animate-pulse align-text-bottom"></span>
</span>
