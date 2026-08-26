<script lang="ts">
  import { t } from "svelte-i18n";
  import type { Assertion, Citation } from "$lib/chat/confidenceParser";
  import { Button } from "$lib/components/ui/button";

  interface Props {
    /** The raw text span owned by this assertion. */
    text: string;
    assertion: Assertion;
    /** Citations already resolved to objects (unknown ids stripped). */
    citations: Citation[];
    /** 1-based footnote numbers indexed by citation id. */
    footnoteNumbers: Map<string, number>;
    /** Currently hovered citation id (bidirectional highlight). */
    hoveredId: string | null;
    onCitationHover: (id: string | null) => void;
    onCitationClick: (id: string) => void;
  }

  let {
    text,
    assertion,
    citations,
    footnoteNumbers,
    hoveredId,
    onCitationHover,
    onCitationClick,
  }: Props = $props();

  const levelClass = $derived(
    assertion.confidence === "high"
      ? "bg-success/10 text-success border-success/30"
      : assertion.confidence === "medium"
        ? "bg-warning/10 text-warning border-warning/30"
        : "bg-destructive/10 text-destructive border-destructive/30",
  );

  const highlight = $derived(
    hoveredId !== null && assertion.citation_ids.includes(hoveredId)
      ? "ring-1 ring-primary/60 bg-primary/5"
      : "",
  );
</script>

<!-- Inline assertion span: coloured underline + confidence dot + footnote numbers.
     Bidirectional highlight: when a citation number is hovered elsewhere, the
     owning span lifts its ring, and vice-versa. -->
<span
  class="inline-baseline rounded-[3px] px-0.5 transition-colors {highlight}"
  data-assertion-citations={assertion.citation_ids.join(",")}
>
  <span
    class="inline-flex items-baseline gap-1 border-b border-dotted {levelClass.replace('bg-', 'decoration-')}"
    aria-label={$t("chat.assertion_confidence_aria", {
      values: { level: assertion.confidence },
    })}
  >
    <span
      class="inline-block h-1.5 w-1.5 rounded-full border {levelClass}"
      aria-hidden="true"
    ></span>
    <span>{text}</span>
  </span>
  {#each assertion.citation_ids as cid, idx (cid)}
    {@const n = footnoteNumbers.get(cid)}
    {#if n !== undefined}
      {@const cite = citations.find((c) => c.id === cid)}
      <sup class="ml-px">
        <Button variant="ghost" size="sm"
          type="button"
          class="inline-flex h-3.5 min-w-3.5 items-center justify-center rounded-full border border-border/50 bg-card/70 px-[3px] text-micro-xs font-medium text-muted-foreground hover:bg-primary/10 hover:text-primary focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/50 {hoveredId === cid ? 'bg-primary/15 text-primary ring-1 ring-primary/50' : ''}"
          onmouseenter={() => onCitationHover(cid)}
          onmouseleave={() => onCitationHover(null)}
          onfocus={() => onCitationHover(cid)}
          onblur={() => onCitationHover(null)}
          onclick={() => onCitationClick(cid)}
          title={cite?.title ?? cid}
          aria-label={$t("chat.assertion_citation_aria", {
            values: { number: n, title: cite?.title ?? cid },
          })}
          data-testid="assertion-cite-{cid}"
        >
          {n}
        </Button>
      </sup>
      {#if idx < assertion.citation_ids.length - 1}<span class="sr-only">,</span>{/if}
    {/if}
  {/each}
</span>
