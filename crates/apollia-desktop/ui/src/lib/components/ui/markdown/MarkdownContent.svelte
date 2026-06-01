<script lang="ts">
  import { renderMarkdown } from "$lib/utils/markdown";
  import { hydrateCodeBlocks } from "$lib/utils/shikiHydrator";
  import "./markdown-prose.css";

  interface Props {
    content: string;
    class?: string;
  }

  let { content, class: className = "" }: Props = $props();

  const rendered = $derived(renderMarkdown(content));

  let container = $state<HTMLDivElement | undefined>();

  // Shiki hydration - fires after every render pass.  Idempotent: blocks
  // already highlighted (class `apollia-code-hi`) are skipped so streaming
  // content does not double-highlight.
  $effect(() => {
    void rendered; // depend on rendered output
    if (!container) return;
    void hydrateCodeBlocks(container);
  });

  async function handleClick(event: MouseEvent): Promise<void> {
    const target = event.target as HTMLElement;
    const copyBtn = target.closest("[data-copy-code]") as HTMLElement | null;
    if (!copyBtn) return;

    const code = copyBtn.dataset.code;
    if (!code) return;

    try {
      await navigator.clipboard.writeText(decodeURIComponent(code));
      copyBtn.classList.add("apollia-code-copied");
      setTimeout(() => copyBtn.classList.remove("apollia-code-copied"), 1500);
    } catch {
      // clipboard API may not be available
    }
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  bind:this={container}
  class="apollia-prose {className}"
  onclick={handleClick}
>
  {@html rendered}
</div>
