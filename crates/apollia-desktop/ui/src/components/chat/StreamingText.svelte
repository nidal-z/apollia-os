<script lang="ts">
  import { renderMarkdown } from "$lib/utils/markdown";
  import "$lib/components/ui/markdown/markdown-prose.css";

  interface Props {
    text: string;
  }

  let { text }: Props = $props();

  let debouncedText = $state("");
  let timerId: ReturnType<typeof setTimeout> | null = null;

  $effect(() => {
    // Track `text` reactively
    const current = text;
    if (timerId !== null) clearTimeout(timerId);
    timerId = setTimeout(() => {
      debouncedText = current;
    }, 50);

    return () => {
      if (timerId !== null) clearTimeout(timerId);
    };
  });

  const rendered = $derived(renderMarkdown(debouncedText));

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
  {@html rendered}<span class="inline-block w-0.5 h-4 bg-primary animate-pulse align-text-bottom"></span>
</span>
