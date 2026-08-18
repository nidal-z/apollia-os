<script lang="ts">
  import { ExternalLink, FileText, Wrench, Brain, Globe } from "lucide-svelte";
  import type { Citation } from "$lib/chat/confidenceParser";
  import { handleExternalLinkClick } from "$lib/utils/externalLink";

  interface Props {
    number: number;
    citation: Citation;
    hoveredId: string | null;
    onHover: (id: string | null) => void;
  }

  let { number, citation, hoveredId, onHover }: Props = $props();

  const icon = $derived(
    citation.source_type === "web"
      ? Globe
      : citation.source_type === "document"
        ? FileText
        : citation.source_type === "tool"
          ? Wrench
          : citation.source_type === "memory"
            ? Brain
            : ExternalLink,
  );

  const highlighted = $derived(hoveredId === citation.id);
</script>

<li
  class="flex gap-2 rounded-md border border-border/40 bg-card/40 px-2 py-1.5 text-[11px] transition-colors {highlighted
    ? 'border-primary/60 bg-primary/5'
    : 'hover:bg-card/70'}"
  onmouseenter={() => onHover(citation.id)}
  onmouseleave={() => onHover(null)}
  id="citation-{citation.id}"
  data-testid="citation-footnote-{citation.id}"
>
  <span
    class="mt-[1px] inline-flex h-[14px] min-w-[14px] items-center justify-center rounded-full bg-muted text-[9px] font-medium text-muted-foreground"
    aria-hidden="true"
  >
    {number}
  </span>
  <div class="flex min-w-0 flex-1 flex-col gap-0.5">
    <div class="flex items-center gap-1 text-foreground">
      {#key citation.source_type}
        {@const Icon = icon}
        <Icon size={11} class="shrink-0 text-muted-foreground" />
      {/key}
      {#if citation.url}
        <a
          href={citation.url}
          target="_blank"
          rel="noopener noreferrer"
          onclick={handleExternalLinkClick}
          class="truncate font-medium hover:underline"
        >
          {citation.title}
        </a>
      {:else}
        <span class="truncate font-medium">{citation.title}</span>
      {/if}
    </div>
    {#if citation.excerpt}
      <p class="line-clamp-2 text-muted-foreground/80">{citation.excerpt}</p>
    {/if}
  </div>
</li>
