<script lang="ts" module>
  import type { ChatViewMode } from "$lib/stores/chatAppearance";

  /**
   * How an answer is shown. "plain" is the text as received, nothing rendered
   * and nothing parsed out of it; otherwise Markdown, unless the text carries
   * confidence or citation markers, which have their own rendering.
   */
  export function answerRendering(input: {
    viewMode: ChatViewMode;
    hasMarkup: boolean;
  }): "plain" | "markdown" | "assertions" {
    if (input.viewMode === "plain") return "plain";
    return input.hasMarkup ? "assertions" : "markdown";
  }
</script>

<script lang="ts">
  import { t } from "svelte-i18n";
  import { chatViewMode } from "$lib/stores/chatAppearance";
  import { MarkdownContent } from "$lib/components/ui/markdown";
  import AssertionInline from "./AssertionInline.svelte";
  import CitationFootnote from "./CitationFootnote.svelte";
  import {
    parseConfidence,
    segmentize,
    type Citation,
  } from "$lib/chat/confidenceParser";

  interface Props {
    /** Raw message content (may include [conf:…] / [cite:…] markup). */
    content: string;
    /** Citations referenced by markers. Missing ids are ignored. */
    citations?: Citation[];
  }

  let { content, citations = [] }: Props = $props();

  const parsed = $derived(parseConfidence(content));
  const segments = $derived(segmentize(parsed));

  // Build footnote numbering from the *order* citations are first referenced
  // in the text - stable, and independent of the citations prop order.
  const footnoteNumbers = $derived.by(() => {
    const map = new Map<string, number>();
    let next = 1;
    for (const a of parsed.assertions) {
      for (const id of a.citation_ids) {
        if (!map.has(id)) {
          map.set(id, next++);
        }
      }
    }
    return map;
  });

  const orderedCitations = $derived.by(() => {
    const byId = new Map(citations.map((c) => [c.id, c] as const));
    const out: Citation[] = [];
    // Entries of a Map preserve insertion order → footnote order.
    for (const id of footnoteNumbers.keys()) {
      const c = byId.get(id);
      if (c) out.push(c);
    }
    return out;
  });

  let hoveredId = $state<string | null>(null);

  function handleCitationClick(id: string): void {
    const el = document.getElementById(`citation-${id}`);
    if (el) {
      el.scrollIntoView({ behavior: "smooth", block: "nearest" });
    }
  }

  // Fast path: no markers at all → delegate to plain Markdown rendering so
  // we don't break code blocks, tables, etc.
  const hasMarkup = $derived(parsed.assertions.length > 0);
  const rendering = $derived(answerRendering({ viewMode: $chatViewMode, hasMarkup }));
</script>

{#if rendering === "plain"}
  <p class="whitespace-pre-wrap break-words" data-testid="message-plain">{content}</p>
{:else if rendering === "markdown"}
  <MarkdownContent {content} />
{:else}
  <div class="space-y-2" data-testid="message-renderer">
    <p class="whitespace-pre-wrap break-words">
      {#each segments as seg, i (i)}
        {#if seg.assertion}
          <AssertionInline
            text={seg.text}
            assertion={seg.assertion}
            citations={orderedCitations}
            {footnoteNumbers}
            {hoveredId}
            onCitationHover={(id) => (hoveredId = id)}
            onCitationClick={handleCitationClick}
          />
        {:else}
          <span>{seg.text}</span>
        {/if}
      {/each}
    </p>

    {#if orderedCitations.length > 0}
      <aside
        class="rounded-md border border-border/40 bg-muted/20 p-2"
        aria-label={$t("chat.citations_aria")}
      >
        <p class="mb-1 text-micro font-medium uppercase tracking-wide text-muted-foreground">
          {$t("chat.activity.sources")}
        </p>
        <ol class="space-y-1">
          {#each orderedCitations as c (c.id)}
            {@const n = footnoteNumbers.get(c.id) ?? 0}
            <CitationFootnote
              number={n}
              citation={c}
              {hoveredId}
              onHover={(id) => (hoveredId = id)}
            />
          {/each}
        </ol>
      </aside>
    {/if}
  </div>
{/if}
