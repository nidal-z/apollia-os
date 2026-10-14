<script lang="ts">
  /**
   * SheetSection - one titled section of an agent sheet: a title, marked
   * "Modified" when its value is the user's rather than the package's, a
   * one-line legend saying what the section is for, an optional action on the
   * right, and its body, all in one card.
   */
  import type { Snippet } from "svelte";
  import { t } from "svelte-i18n";
  import { Badge } from "$lib/components/ui/badge";

  interface Props {
    title: string;
    legend: string;
    testid: string;
    /** Whether the value shown is the user's rather than the package's. */
    modified?: boolean;
    action?: Snippet;
    children: Snippet;
  }

  let { title, legend, testid, modified = false, action, children }: Props = $props();
</script>

<!-- The grammar of a settings section: a card whose head names the section
     and says what it is for, over its body. -->
<section
  class="overflow-hidden rounded-xl border border-border bg-card shadow-sm"
  data-testid={testid}
  aria-label={title}
>
  <!-- The actions wrap under the title when the card is too narrow for both:
       held on the same line, wide buttons left the legend one word wide and
       ran past the edge of the card. -->
  <div class="flex flex-wrap items-start justify-between gap-3 border-b border-border/60 px-4 py-3">
    <div class="min-w-48 flex-1">
      <div class="flex items-center gap-2">
        <h3 class="m-0 text-overline uppercase text-muted-foreground">{title}</h3>
        {#if modified}
          <Badge size="sm" variant="warning" data-testid={`${testid}-modified`}>{$t("agent_adjust.modified")}</Badge>
        {/if}
      </div>
      <p class="m-0 mt-0.5 text-caption text-muted-foreground">{legend}</p>
    </div>
    {#if action}
      <div class="flex max-w-full flex-wrap justify-end gap-2">{@render action()}</div>
    {/if}
  </div>
  <div class="p-4">
    {@render children()}
  </div>
</section>
