<script lang="ts">
  /**
   * DashboardCardHeader - shared header row for a bento card.
   *
   * Local to the Dashboard redesign. Renders the uppercase card title, a
   * tabular count, and an optional "see all" link. FLAGGED for Phase-0.5:
   * candidate to promote next to SectionTitle as a card-scoped header.
   */
  interface Props {
    title: string;
    count: number;
    /** Dense header (no bottom hairline, tighter padding) for secondary cards. */
    dense?: boolean;
    /** When set, renders the trailing "see all" link. */
    onSeeAll?: () => void;
    seeAllLabel?: string;
    seeAllAria?: string;
    seeAllTestid?: string;
  }

  let {
    title,
    count,
    dense = false,
    onSeeAll,
    seeAllLabel,
    seeAllAria,
    seeAllTestid,
  }: Props = $props();
</script>

<div
  class="flex items-baseline justify-between {dense
    ? 'px-4 pt-3.5 pb-2'
    : 'px-5 pt-4 pb-3 border-b border-border/40'}"
>
  <div class="flex items-baseline gap-2">
    <h3 class="m-0 text-overline uppercase font-mono text-muted-foreground">
      {title}
    </h3>
    <span class="text-caption font-mono text-muted-foreground/70 tabular-nums">
      {count}
    </span>
  </div>
  {#if onSeeAll}
    <button
      type="button"
      class="text-caption text-primary hover:text-primary/80 transition-colors rounded focus:outline-none focus-visible:ring-2 focus-visible:ring-primary/50"
      onclick={onSeeAll}
      aria-label={seeAllAria}
      data-testid={seeAllTestid}
    >
      {seeAllLabel}
    </button>
  {/if}
</div>
