<script lang="ts">
  /**
   * SidebarGroup — labelled section header + item container (US-SP42-079).
   *
   * The header is visually rendered as a tiny uppercase caption when the
   * sidebar is expanded, and collapses to a screen-reader-only <h2> when
   * the sidebar is in icon mode (a11y: keep the group landmark regardless).
   */
  import type { Snippet } from "svelte";

  interface Props {
    label: string;
    collapsed: boolean;
    testId?: string;
    children: Snippet;
  }

  let { label, collapsed, testId, children }: Props = $props();
</script>

<section
  class="flex flex-col gap-0.5"
  aria-label={label}
  data-testid={testId}
>
  {#if collapsed}
    <h2 class="sr-only">{label}</h2>
  {:else}
    <h2
      class="mb-1 mt-3 px-3 text-[11px] font-semibold uppercase tracking-wider text-muted-foreground/60"
    >
      {label}
    </h2>
  {/if}
  {@render children()}
</section>
