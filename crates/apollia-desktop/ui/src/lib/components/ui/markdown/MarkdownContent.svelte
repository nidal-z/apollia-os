<script lang="ts">
  import {
    decorateCodeBlocks,
    handleMarkdownLinkClick,
    renderMarkdown,
    type MarkdownProfile,
  } from "$lib/utils/markdown";
  import { hydrateCodeBlocks } from "$lib/utils/shikiHydrator";
  import "./markdown-prose.css";

  interface Props {
    content: string;
    class?: string;
    /** What is rendered: the output of a model, or a page of the documentation. */
    profile?: MarkdownProfile;
    /**
     * Takes every link of the content, with the address as it is written.
     * Without it, a link leaves for the browser.
     */
    onlink?: (href: string) => void;
    /** Runs on the mounted content after every render, once the code blocks are decorated. */
    ondecorate?: (root: HTMLElement) => void;
  }

  let {
    content,
    class: className = "",
    profile = "chat",
    onlink,
    ondecorate,
  }: Props = $props();

  const rendered = $derived(renderMarkdown(content, profile));

  let container = $state<HTMLDivElement | undefined>();

  // Code-block decoration then Shiki hydration - fires after every render
  // pass. The decoration grafts the wrapper, the language label and the copy
  // button onto the sanitized DOM (`renderMarkdown` emits rendering tags
  // only, so nothing interactive can come from the model), and hydration
  // swaps in the highlighted markup. Both are idempotent: decorated blocks
  // and already highlighted ones (class `apollia-code-hi`) are skipped so
  // streaming content does not double-process.
  $effect(() => {
    void rendered; // depend on rendered output
    if (!container) return;
    decorateCodeBlocks(container);
    ondecorate?.(container);
    void hydrateCodeBlocks(container);
  });

  /** Hands a clicked link to `onlink`, so the webview never follows it itself. */
  function routeLink(event: MouseEvent): boolean {
    if (!onlink) return false;
    const anchor = (event.target as Element | null)?.closest?.("a[href]");
    if (!anchor) return false;
    event.preventDefault();
    onlink(anchor.getAttribute("href") ?? "");
    return true;
  }

  async function handleClick(event: MouseEvent): Promise<void> {
    // Outbound links first: the anchors are injected markup, so this
    // container is the only place that can route them to the opener. The
    // copy-button branch below returns early and would swallow the click.
    if (routeLink(event)) return;
    if (handleMarkdownLinkClick(event)) return;

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
